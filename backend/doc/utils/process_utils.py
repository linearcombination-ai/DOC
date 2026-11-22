"""
Helpers for running external commands (git, curl) with a timeout, with
argument lists rather than shell strings, and with bounded retry/wait
when the failure looks transient.
"""

import re
import subprocess
import time
from typing import Callable, Mapping, Optional, Sequence

from doc.config import settings

logger = settings.logger(__name__)

# Fragments of git/curl diagnostics that mean waiting and retrying cannot
# help: the repository or branch does not exist, or we are not allowed.
_PERMANENT_GIT_PATTERNS = (
    "repository not found",
    "does not exist",
    "authentication failed",
    "could not read username",
    "could not read password",
)
_GIT_PERMANENT_RE = re.compile(
    r"repository '[^']*' not found"
    r"|remote branch .* not found in upstream"
    r"|returned error: 40[134]"
)
# curl --fail reports "The requested URL returned error: 404". 4xx other
# than 408 (request timeout) and 429 (too many requests) are permanent.
_CURL_PERMANENT_RE = re.compile(r"returned error: 4(?!08|29)\d\d")
_CURL_FAIL_EXIT_CODE = 22


class CommandFailed(Exception):
    """
    A command did not succeed. transient is True if the failure looked
    temporary but we ran out of time (the deadline) before it cleared.
    """

    def __init__(self, command: Sequence[str], transient: bool, detail: str) -> None:
        super().__init__(detail)
        self.command = list(command)
        self.transient = transient
        self.detail = detail


def deadline_from_now(ceiling_seconds: float) -> float:
    """Absolute time.monotonic() value that is ceiling_seconds from now."""
    return time.monotonic() + ceiling_seconds


def is_transient_git_failure(
    result: subprocess.CompletedProcess[str],
) -> bool:
    stderr = (result.stderr or "").lower()
    if _GIT_PERMANENT_RE.search(stderr):
        return False
    return not any(pattern in stderr for pattern in _PERMANENT_GIT_PATTERNS)


def is_transient_curl_failure(
    result: subprocess.CompletedProcess[str],
) -> bool:
    if result.returncode == _CURL_FAIL_EXIT_CODE:
        return _CURL_PERMANENT_RE.search(result.stderr or "") is None
    return True


def run_with_retry(
    command: Sequence[str],
    *,
    per_attempt_timeout: float,
    deadline: float,
    wait_seconds: float,
    is_transient: Callable[[subprocess.CompletedProcess[str]], bool],
    cleanup: Optional[Callable[[], None]] = None,
    env: Optional[Mapping[str, str]] = None,
    run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
    sleep: Callable[[float], None] = time.sleep,
    monotonic: Callable[[], float] = time.monotonic,
) -> subprocess.CompletedProcess[str]:
    """
    Run command (an argument list, never a shell string) until it
    succeeds. A failure that looks transient (timeout, network error,
    server error) is waited out and retried with a growing pause until
    deadline (an absolute monotonic time) passes. A permanent failure
    (as decided by is_transient) is raised at once. Every failure is
    logged. Raises CommandFailed when giving up, so a failed command is
    never mistaken for success.
    """
    attempt = 0
    last_detail = "deadline reached before the command could run"
    while True:
        remaining = deadline - monotonic()
        if remaining <= 0:
            logger.error(
                "Giving up on %s after %s attempt(s): %s",
                command[0],
                attempt,
                last_detail,
            )
            raise CommandFailed(command, True, last_detail)
        attempt += 1
        timeout = min(per_attempt_timeout, remaining)
        try:
            result = run(
                list(command),
                capture_output=True,
                text=True,
                timeout=timeout,
                env=None if env is None else dict(env),
            )
            if result.returncode == 0:
                return result
            last_detail = "exit code {}: {}".format(
                result.returncode, (result.stderr or "").strip()[-500:]
            )
            transient = is_transient(result)
        except subprocess.TimeoutExpired:
            last_detail = "timed out after {:.0f}s".format(timeout)
            transient = True
        except OSError as e:  # e.g. executable missing, not worth retrying
            logger.error("Unable to execute %s: %s", command[0], e)
            raise CommandFailed(command, False, str(e)) from e
        if cleanup is not None:
            cleanup()
        if not transient:
            logger.error(
                "%s failed permanently (attempt %s): %s",
                command[0],
                attempt,
                last_detail,
            )
            raise CommandFailed(command, False, last_detail)
        pause = min(wait_seconds * attempt, max(deadline - monotonic(), 0))
        logger.warning(
            "%s failed transiently (attempt %s), %s. Retrying in %.1fs",
            command[0],
            attempt,
            last_detail,
            pause,
        )
        sleep(pause)
