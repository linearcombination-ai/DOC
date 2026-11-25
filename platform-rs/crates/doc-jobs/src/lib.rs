//! doc-jobs (P5): the job runner that would replace Celery for DOC's document path.
//!
//! * [`model`]: the legacy status strings and the failure result.
//! * [`store`], [`redis_store`], [`mem_store`]: the queue and job state, on Redis Streams or in memory.
//! * [`runner`]: claim, lease, retry with jittered backoff, stall recovery, graceful drain.
//! * [`pipeline`]: DOC's docx job as stages; [`seam`]: the Python docx worker; [`output`]: atomic placement.
//! * [`api`]: the HTTP routes.

pub mod api;
pub mod mem_store;
pub mod model;
pub mod output;
pub mod pipeline;
pub mod redis_store;
pub mod retry;
pub mod runner;
pub mod seam;
pub mod store;
pub mod lock;
pub mod metrics;
pub mod pool;
pub mod html;
pub mod core_stage;
pub mod email;
pub mod limits;
pub mod config;
pub mod pdf;
