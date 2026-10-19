//! What a refused ref of a git push means.
//!
//! Door43 refuses a push from an account that may read the repository but not write to it
//! inside an HTTP 200. It unpacks the pack, then its pre-receive hook refuses every ref: the
//! hook's text, `User permission denied for writing.`, arrives on side band 2, and the status
//! report says `ng <ref> pre-receive hook declined` (docs/evidence/door43-live-2026-10-17.md,
//! step 6). libgit2's push returns `Ok` for that; it hands the ref's status to the
//! `push_update_reference` callback and the side-band text to `sideband_progress`.
//!
//! Side band 2 also carries git's own progress. `git receive-pack` resolves the deltas of a
//! pack above its unpack limit (100 objects) before the hook runs, and writes
//! `Resolving deltas:   0% (0/149)\r…\rResolving deltas: 100% (149/149), done.\n` there,
//! because libgit2 never asks it to be quiet. That text is git's, not Door43's, so it never
//! reaches `server_message`.

use crate::error::Door43Error;
use crate::names::RepoRef;

/// Door43's pre-receive hook says this to an account that may not write to the repository.
const NO_WRITE_ACCESS: &str = "User permission denied for writing";

/// `git receive-pack`'s statuses for a ref that another push moved, created, deleted or held
/// after this push read the advertisement. libgit2 checks a fast-forward against the
/// advertisement it just read (`ErrorCode::NotFastForward`), so this is how a race lost on the
/// server comes back. Git before 2.51 says `failed to update ref`, with `cannot lock ref …: is
/// at X but expected Y` on the side band. Git 2.51 and later update refs in a batch and name
/// only the check that failed, with nothing on the side band; Door43 advertises git 2.54
/// (docs/evidence/door43-live-2026-10-17.md, step 6). Their `failed to update refs` means the
/// server could not write the ref at all, which no retry mends, so it is not here.
const BRANCH_MOVED: [&str; 4] = [
    "failed to update ref",
    "incorrect old value provided",
    "reference already exists",
    "reference does not exist",
];

/// Prefixes git and Gitea put before a message. A line that starts with one is never
/// progress.
const MESSAGE_PREFIXES: [&str; 5] = ["error", "warning", "fatal", "hint", "remote"];

/// The error for a ref Door43 refused. `status` is the ref's status from libgit2's
/// `push_update_reference` callback, such as `pre-receive hook declined`; `remote_text` is all
/// that the push's `sideband_progress` callback received.
pub fn refused_push(repo: &RepoRef, status: &str, remote_text: &str) -> Door43Error {
    let route = format!("POST /{repo}.git/git-receive-pack");
    let said: Vec<&str> = remote_text
        .split(['\r', '\n'])
        .filter(|line| !is_git_progress(line))
        .map(|line| line.trim().trim_start_matches("error:").trim())
        .filter(|line| !line.is_empty())
        .collect();
    if said.iter().any(|line| line.contains(NO_WRITE_ACCESS)) {
        return Door43Error::NoWriteAccess {
            route,
            server_message: said.join("\n"),
        };
    }
    let status = status.trim();
    let server_message = if said.is_empty() {
        status.to_owned()
    } else {
        format!("{} ({status})", said.join("\n"))
    };
    if BRANCH_MOVED.contains(&status) {
        return Door43Error::RefMoved {
            route,
            server_message,
        };
    }
    Door43Error::RefRejected {
        route,
        server_message,
    }
}

/// Whether `line` is one update of git's progress meter: `<title>: <counter>`, where the
/// counter is `<n>% (<done>/<total>)` or `<n>`, followed by nothing, `, done.` or a
/// throughput. The title is translated on a server that runs git in another language, so
/// only the counter's shape is matched.
fn is_git_progress(line: &str) -> bool {
    let line = line.trim();
    match line.split_once(": ") {
        Some((title, counter)) => {
            !MESSAGE_PREFIXES.contains(&title.trim().to_ascii_lowercase().as_str())
                && is_counter(counter)
        }
        // Git splits a line too wide for the terminal after the title, and indents the counter.
        None => line.contains("% (") && is_counter(line),
    }
}

fn is_counter(text: &str) -> bool {
    let (count, rest) = leading_digits(text.trim_start());
    if count.is_empty() {
        return false;
    }
    let rest = match rest.strip_prefix("% (") {
        Some(fraction) => {
            let (done, rest) = leading_digits(fraction);
            let Some(rest) = rest.strip_prefix('/') else {
                return false;
            };
            let (total, rest) = leading_digits(rest);
            match rest.strip_prefix(')') {
                Some(rest) if !done.is_empty() && !total.is_empty() => rest,
                _ => return false,
            }
        }
        None => rest,
    };
    let rest = rest.trim_end();
    rest.is_empty() || rest.starts_with(", ")
}

fn leading_digits(text: &str) -> (&str, &str) {
    text.split_at(text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> RepoRef {
        "birch/test".parse().unwrap()
    }

    /// What `git receive-pack` writes on side band 2 before the hook runs, for a pack of 150
    /// similar blobs (above its unpack limit) pushed by libgit2.
    fn resolving_deltas() -> String {
        let mut text = String::new();
        for done in 0..=149 {
            text.push_str(&format!(
                "Resolving deltas: {:3}% ({done}/149)\r",
                done * 100 / 149
            ));
        }
        text.push_str("Resolving deltas: 100% (149/149), done.\n");
        text
    }

    #[test]
    fn door43s_permission_text_is_no_write_access() {
        let error = refused_push(
            &repo(),
            "pre-receive hook declined",
            "error:\nerror: User permission denied for writing.\nerror:\n",
        );
        assert!(
            matches!(
                &error,
                Door43Error::NoWriteAccess { route, server_message }
                    if route == "POST /birch/test.git/git-receive-pack"
                        && server_message == "User permission denied for writing."
            ),
            "{error:?}"
        );
    }

    #[test]
    fn another_refusal_keeps_door43s_text_and_the_status() {
        let error = refused_push(
            &repo(),
            "pre-receive hook declined",
            "error: Not allowed to push to protected branch main\n",
        );
        assert!(
            matches!(
                &error,
                Door43Error::RefRejected { server_message, .. }
                    if server_message
                        == "Not allowed to push to protected branch main (pre-receive hook declined)"
            ),
            "{error:?}"
        );
        let error = refused_push(&repo(), "funny refname", "");
        assert!(
            matches!(&error, Door43Error::RefRejected { server_message, .. }
                if server_message == "funny refname"),
            "{error:?}"
        );
    }

    #[test]
    fn gits_own_progress_never_reaches_the_server_message() {
        let hook = "error:\nerror: User permission denied for writing.\nerror:\n";
        let error = refused_push(
            &repo(),
            "pre-receive hook declined",
            &format!("{}{hook}", resolving_deltas()),
        );
        assert!(
            matches!(
                &error,
                Door43Error::NoWriteAccess { server_message, .. }
                    if server_message == "User permission denied for writing."
            ),
            "{error:?}"
        );

        let protected = "error:\nerror: Not allowed to push to protected branch main\nerror:\n";
        let error = refused_push(
            &repo(),
            "pre-receive hook declined",
            &format!(
                "{}Checking connectivity: 150, done.\nUnpacking objects:  50% (1/2), 1.20 MiB | \
                 2.00 MiB/s\r\n{protected}",
                resolving_deltas()
            ),
        );
        assert!(
            matches!(
                &error,
                Door43Error::RefRejected { server_message, .. }
                    if server_message
                        == "Not allowed to push to protected branch main (pre-receive hook declined)"
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_hook_line_that_looks_like_a_count_is_kept() {
        let error = refused_push(
            &repo(),
            "pre-receive hook declined",
            "error: 3, as the branch policy says\nwarning: 2\nQuota: 10 GB exceeded\n",
        );
        assert!(
            matches!(
                &error,
                Door43Error::RefRejected { server_message, .. }
                    if server_message
                        == "3, as the branch policy says\nwarning: 2\nQuota: 10 GB exceeded \
                            (pre-receive hook declined)"
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_ref_another_push_moved_is_ref_moved() {
        let error = refused_push(
            &repo(),
            "failed to update ref",
            "error: cannot lock ref 'refs/heads/main': is at \
             1111111111111111111111111111111111111111 but expected \
             2222222222222222222222222222222222222222\n",
        );
        assert!(
            matches!(
                &error,
                Door43Error::RefMoved { route, server_message }
                    if route == "POST /birch/test.git/git-receive-pack"
                        && server_message.starts_with("cannot lock ref 'refs/heads/main'")
                        && server_message.ends_with("(failed to update ref)")
            ),
            "{error:?}"
        );
    }

    /// Git 2.51 and later, as git 2.53 answers a ref that another push moved, created or held
    /// the lock of, or deleted: the status alone, with nothing on the side band.
    #[test]
    fn newer_gits_lost_races_are_ref_moved() {
        for status in [
            "incorrect old value provided",
            "reference already exists",
            "reference does not exist",
        ] {
            let error = refused_push(&repo(), status, "");
            assert!(
                matches!(&error, Door43Error::RefMoved { server_message, .. }
                    if server_message == status),
                "{status}: {error:?}"
            );
        }
    }

    #[test]
    fn a_ref_the_server_cannot_write_is_rejected_with_its_text() {
        let error = refused_push(
            &repo(),
            "failed to update refs",
            "error: cannot lock ref 'refs/heads/main': Unable to create \
             '/srv/birch/test.git/refs/heads/main.lock': Permission denied\n",
        );
        assert!(
            matches!(
                &error,
                Door43Error::RefRejected { server_message, .. }
                    if server_message.starts_with("cannot lock ref 'refs/heads/main'")
                        && server_message.ends_with("Permission denied (failed to update refs)")
            ),
            "{error:?}"
        );
    }
}
