"""Prepared supplemental transformations; no process launch or source-file mutation.

The single authorized executor may import these only after exact-revision GO. A generic
mutant pass is an observation, never a standard mutation detection or validation pass.
"""

from pathlib import Path


BOUNDED_OBSERVATION = "model::random_histories_lose_no_work"
PROBES = {
    "FA": {
        "source": "crates/tc-store/src/classify.rs",
        "generic": "model::oracle_independence_false_adoption",
        "control": "model::oracle_independence_false_adoption_control",
        "marker": "ORACLE_FALSE_ADOPTION_LOST_OUTSIDE_TEXT",
    },
}


def once(source, before, after):
    count = source.count(before)
    if count != 1:
        raise ValueError(f"supplemental anchor count {count}, expected 1: {before!r}")
    return source.replace(before, after)


def append_temporary_tests(source):
    if "fn oracle_independence_" in source:
        raise ValueError("temporary probe tests already present; preserve and inspect")
    tests = Path(__file__).with_name("probes.rs").read_text()
    return source + "\n" + tests


def mutate(probe, source):
    if probe == "FA":
        return once(
            source,
            "                Action::Record(base) => {\n"
            "                    if let Some(bytes) = bytes {\n"
            "                        self.found_outside(rel, bytes, base)?;\n"
            "                    }\n"
            "                    Next::Recorded\n"
            "                }",
            "                Action::Record(base) => {\n"
            "                    if rel == \"ingredients/TIT.usfm\" {\n"
            "                        if let Some(bytes) = bytes {\n"
            "                            self.note(rel, Event::Adopted {\n"
            "                                md5: self.hash(bytes),\n"
            "                                size: bytes.len() as u64,\n"
            "                            });\n"
            "                            self.write_manifest()?;\n"
            "                        }\n"
            "                        Next::Converged\n"
            "                    } else {\n"
            "                        if let Some(bytes) = bytes {\n"
            "                            self.found_outside(rel, bytes, base)?;\n"
            "                        }\n"
            "                        Next::Recorded\n"
            "                    }\n"
            "                }",
        )
    if probe == "ON":
        prefix = "    pub(crate) fn notice(&self, notice: Notice, copy: Option<String>) {"
        return once(
            source,
            prefix,
            prefix + "\n"
            "        if matches!(&notice,\n"
            "            Notice::OutsideEditRecorded { .. }\n"
            "                | Notice::OutsideEditHeld { .. }\n"
            "                | Notice::OutsideEditUnreadable { .. }\n"
            "                | Notice::OutsideRemovalKept { .. }\n"
            "                | Notice::StorageDamage { .. }\n"
            "                | Notice::MetadataEditedOutside { .. }\n"
            "                | Notice::DerivedFieldsEdited { .. }\n"
            "                | Notice::FixedFileRestored { .. })\n"
            "        {\n"
            "            return;\n"
            "        }",
        )
    raise ValueError(probe)


def classification(*, clean_passed, restored_passed, generic, control):
    """Inputs must already satisfy per-command exact libtest/receipt criteria.

    generic: pass, runtime-failure, or invalid. control: marked-runtime-failure or
    invalid. The original random-test observation is deliberately excluded here.
    """
    if not clean_passed or not restored_passed or control != "marked-runtime-failure":
        return "incomplete"
    if generic == "pass":
        return "proof_gap_observed"
    if generic == "runtime-failure":
        return "generic_witness_detected"
    return "incomplete"
