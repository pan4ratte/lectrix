# Local corpus

Real-world PDFs the user owns but cannot publish (AGENTS.md section 9). Only the scripts are
in git; `files/` and `manifest.json` are git-ignored and stay on this machine.

```
cargo build --release -p pdf-cli
python tests/local-corpus/survey.py "<library folder>"   # read-only profile of every PDF
python tests/local-corpus/select.py                      # pick and copy test files
cargo test --release -p pdf-core --test local_corpus -- --ignored --nocapture
powershell -File tests/perf/measure.ps1 -Pdf tests/local-corpus/files/<file>.pdf [-Perf]
```

The library is only read: the survey opens files for reading, and selection copies them
(checking SHA-256). Tests open the copies and write only to `target/test-output/`.
