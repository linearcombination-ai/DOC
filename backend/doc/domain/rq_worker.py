"""
RQ worker class that preloads the job modules.

RQ forks a fresh "work horse" process for every job. If the job's
module has not been imported yet, each horse pays the full import cost
(pydantic, docx, bs4, ...) again. ``rq worker-pool`` imports the class
named by ``--worker-class`` in the parent process before it forks its
workers, so importing the generator modules here means every worker,
and every horse forked from it, inherits them already loaded.

Use with: rq worker-pool -w doc.domain.rq_worker.PreloadedWorker
"""

from rq import Worker

# Imported for their side effect of loading the modules into the
# parent process.
import doc.domain.document_generator  # noqa: F401
import doc.domain.resource_lookup  # noqa: F401
import passages.domain.document_generator  # noqa: F401
import stet.domain.document_generator  # noqa: F401


class PreloadedWorker(Worker):
    """A standard forking Worker; the preloading happens at import."""
