"""Tests that the append-only audit logs merge without conflicts: python3 -I .audit/tools/union_test.py"""
import os, shutil, subprocess, tempfile, unittest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LOGS = [".audit/minors.tsv", ".audit/rebuild.tsv"]


def git(cwd, *args):
    return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid", *args],
                          cwd=cwd, capture_output=True, text=True)


def append(cwd, path, row):
    with open(os.path.join(cwd, path), "a", encoding="utf-8") as f:
        f.write(row + "\n")
    git(cwd, "commit", "-qam", row)


class UnionTest(unittest.TestCase):
    def test_two_branches_appending_to_an_audit_log_merge_without_conflict(self):
        for path in LOGS:
            with self.subTest(path=path), tempfile.TemporaryDirectory() as repo:
                git(repo, "init", "-q", "-b", "base")
                shutil.copy(os.path.join(ROOT, ".gitattributes"), repo)
                os.makedirs(os.path.join(repo, os.path.dirname(path)))
                with open(os.path.join(repo, path), "w", encoding="utf-8") as f:
                    f.write("unit\tfinding\tevidence\nX\tfirst\tnone\n")
                git(repo, "add", "-A")
                git(repo, "commit", "-qm", "base")
                git(repo, "checkout", "-qb", "a")
                append(repo, path, "A\tfrom a\tnone")
                git(repo, "checkout", "-qb", "b", "base")
                append(repo, path, "B\tfrom b\tnone")
                merged = git(repo, "merge", "-q", "--no-edit", "a")
                with open(os.path.join(repo, path), encoding="utf-8") as f:
                    text = f.read()
                ok = merged.returncode == 0 and "A\tfrom a" in text and "B\tfrom b" in text
                self.assertTrue(ok, f"INV-LOGS-NEVER-CONFLICT: {path} conflicted; .gitattributes needs '{path} merge=union'")


if __name__ == "__main__":
    unittest.main()
