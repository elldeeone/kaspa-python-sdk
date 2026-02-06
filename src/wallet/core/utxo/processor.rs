use crate::consensus::core::network::PyNetworkId;
use crate::rpc::wrpc::client::PyRpcClient;
use ahash::AHashMap;
use kaspa_wallet_core::events::EventKind;
use kaspa_wallet_core::rpc::{DynRpcApi, Rpc};
use kaspa_wallet_core::utxo::{
    UtxoProcessor, set_coinbase_transaction_maturity_period_daa,
    set_user_transaction_maturity_period_daa,
};
use pyo3::{
    exceptions::PyException,
    prelude::*,
    types::{PyDict, PyModule, PyTuple},
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

/// UTXO processor coordinating address tracking and UTXO updates.
#[gen_stub_pyclass]
#[pyclass(name = "UtxoProcessor")]
#[derive(Clone)]
pub struct PyUtxoProcessor {
    processor: UtxoProcessor,
    rpc: PyRpcClient,
    callbacks: Arc<Mutex<AHashMap<EventKind, Vec<PyCallback>>>>,
}

impl PyUtxoProcessor {
    pub fn inner(&self) -> &UtxoProcessor {
        &self.processor
    }
}

#[derive(Clone)]
#[allow(dead_code)]
struct PyCallback {
    callback: Arc<Py<PyAny>>,
    args: Option<Arc<Py<PyTuple>>>,
    kwargs: Option<Arc<Py<PyDict>>>,
}

#[allow(dead_code)]
impl PyCallback {
    fn add_event_to_args(&self, py: Python, event: Bound<PyDict>) -> PyResult<Py<PyTuple>> {
        match &self.args {
            Some(existing_args) => {
                let tuple_ref = existing_args.bind(py);
                let mut new_args: Vec<Py<PyAny>> =
                    tuple_ref.iter().map(|arg| arg.unbind()).collect();
                new_args.push(event.into());
                Ok(Py::from(PyTuple::new(py, new_args)?))
            }
            None => Ok(Py::from(PyTuple::new(py, [event])?)),
        }
    }

    fn execute(&self, py: Python, event: Bound<PyDict>) -> PyResult<Py<PyAny>> {
        let args = self.add_event_to_args(py, event)?;
        let kwargs = self.kwargs.as_ref().map(|kw| kw.bind(py));

        self.callback
            .call(py, args.bind(py), kwargs)
            .map_err(|err| {
                let traceback = PyModule::import(py, "traceback")
                    .and_then(|traceback| {
                        traceback.call_method(
                            "format_exception",
                            (err.get_type(py), err.value(py), err.traceback(py)),
                            None,
                        )
                    })
                    .map(|formatted| {
                        let trace_lines: Vec<String> = formatted
                            .extract()
                            .unwrap_or_else(|_| vec!["<Failed to retrieve traceback>".to_string()]);
                        trace_lines.join("")
                    })
                    .unwrap_or_else(|_| "<Failed to retrieve traceback>".to_string());

                PyException::new_err(traceback.to_string())
            })
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyUtxoProcessor {
    /// Create a new UtxoProcessor.
    ///
    /// Args:
    ///     rpc: The RPC client to use for network communication.
    ///     network_id: Network identifier for UTXO processing.
    #[new]
    pub fn ctor(rpc: PyRpcClient, network_id: PyNetworkId) -> PyResult<Self> {
        let rpc_api: Arc<DynRpcApi> = rpc.client().clone();
        let rpc_ctl = rpc.client().rpc_ctl().clone();
        let rpc_binding = Rpc::new(rpc_api, rpc_ctl);

        let processor = UtxoProcessor::new(Some(rpc_binding), Some(network_id.into()), None, None);

        Ok(Self {
            processor,
            rpc,
            callbacks: Arc::new(Mutex::new(Default::default())),
        })
    }

    /// Start UTXO processing (async).
    fn start<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let processor = self.processor.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            processor
                .start()
                .await
                .map_err(|err| PyException::new_err(err.to_string()))?;
            Ok(())
        })
    }

    /// Stop UTXO processing (async).
    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let processor = self.processor.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            processor
                .stop()
                .await
                .map_err(|err| PyException::new_err(err.to_string()))?;
            Ok(())
        })
    }

    /// The associated RPC client.
    #[getter]
    pub fn get_rpc(&self) -> PyRpcClient {
        self.rpc.clone()
    }

    /// The network id used by the processor (if set).
    #[getter]
    pub fn get_network_id(&self) -> Option<PyNetworkId> {
        self.processor.network_id().ok().map(PyNetworkId::from)
    }

    /// Set the network id for the processor.
    pub fn set_network_id(&self, network_id: PyNetworkId) {
        self.processor.set_network_id(&network_id.into());
    }

    /// Set the coinbase transaction maturity period DAA for a network.
    #[staticmethod]
    pub fn set_coinbase_transaction_maturity_daa(network_id: PyNetworkId, value: u64) {
        let network_id = network_id.into();
        set_coinbase_transaction_maturity_period_daa(&network_id, value);
    }

    /// Set the user transaction maturity period DAA for a network.
    #[staticmethod]
    pub fn set_user_transaction_maturity_daa(network_id: PyNetworkId, value: u64) {
        let network_id = network_id.into();
        set_user_transaction_maturity_period_daa(&network_id, value);
    }

    /// Whether the processor is connected and running.
    #[getter]
    pub fn get_is_active(&self) -> bool {
        self.processor
            .try_rpc_ctl()
            .map(|ctl| ctl.is_connected())
            .unwrap_or(false)
            && self.processor.is_connected()
            && self.processor.is_running()
    }

    /// Register a callback for UtxoProcessor events.
    ///
    /// Args:
    ///     event_or_callback: Event target as string (kebab-case), a list of strings, "*" / "all", or a callback (listen to all events).
    ///     callback: Function to call when event occurs (required when event_or_callback is an event target).
    ///     *args: Additional arguments to pass to callback.
    ///     **kwargs: Additional keyword arguments to pass to callback.
    ///
    /// Notes:
    ///     Callback will be invoked as: callback(*args, event, **kwargs)
    ///     Where event is a dict like: {"type": str, "data": ...}
    #[pyo3(signature = (event_or_callback, callback=None, *args, **kwargs))]
    fn add_event_listener(
        &self,
        py: Python,
        event_or_callback: Bound<'_, PyAny>,
        callback: Option<Py<PyAny>>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let (targets, callback) = match callback {
            Some(callback) => (parse_event_targets(event_or_callback)?, callback),
            None => {
                if event_or_callback.is_callable() {
                    (
                        vec![EventKind::All],
                        event_or_callback.extract::<Py<PyAny>>()?,
                    )
                } else {
                    return Err(PyException::new_err(
                        "Expected `str | Sequence[str]` for event_or_callback and `callback` to be callable",
                    ));
                }
            }
        };

        let args = args.into_pyobject(py)?.extract::<Py<PyTuple>>()?;
        let kwargs = match kwargs {
            Some(kw) => kw.into_pyobject(py)?.extract::<Py<PyDict>>()?,
            None => PyDict::new(py).into(),
        };

        let py_callback = PyCallback {
            callback: Arc::new(callback),
            args: Some(Arc::new(args)),
            kwargs: Some(Arc::new(kwargs)),
        };

        let mut callbacks = self.callbacks.lock().unwrap();
        for target in targets {
            callbacks
                .entry(target)
                .or_default()
                .push(py_callback.clone());
        }
        Ok(())
    }

    /// Remove an event listener.
    ///
    /// Args:
    ///     event_or_callback: Event target as string (kebab-case), a list of strings, "*" / "all", or a callback (remove from all events).
    ///     callback: Specific callback to remove, or None to remove all callbacks for the event target(s).
    #[pyo3(signature = (event_or_callback, callback=None))]
    fn remove_event_listener(
        &self,
        event_or_callback: Bound<'_, PyAny>,
        callback: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let mut callbacks = self.callbacks.lock().unwrap();

        if callback.is_none() && event_or_callback.is_callable() {
            let callback = event_or_callback.extract::<Py<PyAny>>()?;
            for handlers in callbacks.values_mut() {
                handlers.retain(|entry| entry.callback.as_ref().as_ptr() != callback.as_ptr());
            }
            return Ok(());
        }

        let targets = parse_event_targets(event_or_callback)?;
        if targets.contains(&EventKind::All) {
            match callback {
                Some(callback) => {
                    for handlers in callbacks.values_mut() {
                        handlers
                            .retain(|entry| entry.callback.as_ref().as_ptr() != callback.as_ptr());
                    }
                }
                None => {
                    callbacks.clear();
                }
            }
            return Ok(());
        }

        match callback {
            Some(callback) => {
                for target in targets {
                    if let Some(handlers) = callbacks.get_mut(&target) {
                        handlers
                            .retain(|entry| entry.callback.as_ref().as_ptr() != callback.as_ptr());
                    }
                }
            }
            None => {
                for target in targets {
                    callbacks.remove(&target);
                }
            }
        }

        Ok(())
    }

    /// Remove all registered event listeners.
    fn remove_all_event_listeners(&self) -> PyResult<()> {
        self.callbacks.lock().unwrap().clear();
        Ok(())
    }
}

fn parse_event_targets(value: Bound<'_, PyAny>) -> PyResult<Vec<EventKind>> {
    if let Ok(s) = value.extract::<String>() {
        return Ok(vec![parse_event_kind(&s)?]);
    }

    let iter = value
        .try_iter()
        .map_err(|_| PyException::new_err("event target must be a str or sequence of str"))?;

    iter.map(|item| {
        let item = item?;
        let s = item
            .extract::<String>()
            .map_err(|_| PyException::new_err("event target must be a str or sequence of str"))?;
        parse_event_kind(&s)
    })
    .collect()
}

fn parse_event_kind(s: &str) -> PyResult<EventKind> {
    if s == "all" {
        return Ok(EventKind::All);
    }
    EventKind::from_str(s).map_err(|err| PyException::new_err(err.to_string()))
}
