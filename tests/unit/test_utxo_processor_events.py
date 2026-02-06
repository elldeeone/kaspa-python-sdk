import pytest

from kaspa import NetworkId, Resolver, RpcClient, UtxoProcessor


def test_add_event_listener_all_overload_smoke():
    client = RpcClient(resolver=Resolver(), network_id="testnet-10")
    processor = UtxoProcessor(client, NetworkId("testnet-10"))

    def cb(event):
        _ = event

    processor.add_event_listener(cb)
    processor.remove_event_listener(cb)


def test_add_event_listener_specific_event_smoke():
    client = RpcClient(resolver=Resolver(), network_id="testnet-10")
    processor = UtxoProcessor(client, NetworkId("testnet-10"))

    def cb(event):
        _ = event

    processor.add_event_listener("connect", cb)
    processor.remove_event_listener("connect", cb)


def test_add_event_listener_multiple_targets_smoke():
    client = RpcClient(resolver=Resolver(), network_id="testnet-10")
    processor = UtxoProcessor(client, NetworkId("testnet-10"))

    def cb(event):
        _ = event

    processor.add_event_listener(["connect", "disconnect"], cb)
    processor.remove_event_listener(["connect", "disconnect"], cb)


def test_remove_all_event_listeners_smoke():
    client = RpcClient(resolver=Resolver(), network_id="testnet-10")
    processor = UtxoProcessor(client, NetworkId("testnet-10"))

    def cb(event):
        _ = event

    processor.add_event_listener("connect", cb)
    processor.remove_all_event_listeners()


def test_add_event_listener_invalid_target_raises():
    client = RpcClient(resolver=Resolver(), network_id="testnet-10")
    processor = UtxoProcessor(client, NetworkId("testnet-10"))

    def cb(event):
        _ = event

    with pytest.raises(Exception):
        processor.add_event_listener("not-a-real-event", cb)

