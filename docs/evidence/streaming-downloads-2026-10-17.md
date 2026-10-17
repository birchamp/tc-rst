# Streaming downloads (unit A11), measured

What streaming a Door43 download to a file saves, and what it costs. Unit A11 of
`docs/APP-DESIGN.md` §10.

- **Commit:** `2423123` (branch `p0-a11`).
- **Machine:** this container, Intel Xeon @ 2.10 GHz, 4 vCPUs shared with other builds, with
  SHA and AVX-512 instructions. Not the reference laptop of `docs/PERFORMANCE.md`.
- **Build:** release.
- **Program:** `cargo run --release -p tc-door43 --example download_bench -- 20`.

## Method

The fake Door43 (`tc-door43-fake`) serves the burrito export of the largest bundled resource,
`bundle/en/en_ust_v91.sb.zip`: 68 entries, 93,737,880 bytes unpacked. The fake's export of it is
8,643,984 bytes zipped. Each download runs in a process of its own, so the peak resident memory
(`VmHWM`) is that of one download. Two ways are compared, 20 runs each:

- **stream:** the client as it is, writing the zip to a file and checking it from there.
- **memory:** the body read whole into one buffer and checked there, as the client did before
  A11.

The fake builds each export before it answers, deflating all 93.7 MB again, so the whole
download time is mostly the fake's. The client's own time is measured from the first byte of
the answer to the checked export.

## Results

| Way | Whole download, median / p95 | First byte to checked, median / p95 | Peak resident memory, median / max |
|---|---:|---:|---:|
| stream | 2,687 ms / 2,880 ms | 379.0 ms / 405 ms | 104.8 MiB / 106.2 MiB |
| memory | 2,696 ms / 2,897 ms | 370.6 ms / 398 ms | 113.3 MiB / 114.0 MiB |

## What the numbers say

- Streaming lowers the peak by 8.5 MiB: the zip's own size (8.2 MiB) and the buffer's
  slack. It costs about 8 ms (2 %) of the client's time, for writing the file and reading it
  back.
- The rest of the peak is the unpacked content. The check returns every file's bytes in memory
  (93.7 MB here), and tc-resources holds them until the store has written them. Unpacking each
  checked entry straight into the store's staging folder would bound a download's memory by its
  largest file. That is not part of A11; the design lists it as open (§4.6).
- Most of the client's 379 ms is likely the check (unzip, CRC and MD5 of 93.7 MB), since
  loopback carries 8.6 MB in tens of milliseconds; the two were not timed apart. The check runs
  on the blocking pool, so it does not hold up the server's async tasks.
