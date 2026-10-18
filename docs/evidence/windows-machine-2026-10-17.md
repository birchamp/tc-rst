# The owner's Windows computer

Every Windows measurement and test run in `docs/evidence/` that names this computer ran here.
Recorded 2026-10-17.

| | |
|---|---|
| Windows | Windows 10 Pro 22H2, build 19045.6466 |
| CPU | Intel Core i5-8350U (8th generation, Kaby Lake R), 1.70 GHz base |
| Cores and threads | 4 cores, 8 threads |
| SHA instructions | no |
| AVX-512 | no (AVX2: yes; AES-NI: yes) |
| Memory | 8 GB (7.9 GiB visible to Windows) |
| Disk | WD Blue 3D NAND 1 TB (WDS100T2B0B), an M.2 SATA SSD behind the Intel RST controller; 368 GB free of 930 GB |
| Defender | real-time protection on (antimalware client 4.18.26090.9) |
| Toolchain | Rust 1.99.1 (`x86_64-pc-windows-msvc`), Visual Studio 2022 Build Tools (MSVC), Node.js 22.23.2, Git for Windows 2.55.0 |

The SHA and AVX-512 rows come from Rust's `is_x86_feature_detected!` run on this CPU, and
agree with Intel's specification for the i5-8350U.

This computer matches the reference machine of `docs/PERFORMANCE.md`: 4 cores, 8 GB, a SATA
SSD, Windows 10 with Defender real-time protection on, and no SHA instructions. Its numbers
can show that a budget is met.

Git for Windows was installed with its default `core.autocrlf=true`, so the checkout has
CRLF line endings in its text files, as most Windows developers' checkouts do.

Defender excludes no folder: none was added for this work, and the owner confirmed on
2026-10-18 that there are none (only an administrator can read the list).
