use pyo3::prelude::*;

#[pymodule]
fn kaspa(m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();

    Ok(())
}