import threading

from app.main import scheduler_loop


def test_scheduler_loop_runs_until_stopped():
    calls = []

    def run():
        calls.append(1)
        if len(calls) >= 3:
            stop.set()

    stop = threading.Event()
    scheduler_loop(stop, 0.01, run)
    assert len(calls) >= 3


def test_scheduler_loop_swallows_exceptions():
    calls = []

    def run():
        calls.append(1)
        if len(calls) >= 2:
            stop.set()
        raise RuntimeError("boom")

    stop = threading.Event()
    scheduler_loop(stop, 0.01, run)
    assert len(calls) >= 2
