from doc.domain.worker import app

@app.task(name="p5.noop")
def noop() -> int:
    return 1
