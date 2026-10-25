# Evaluating maqam recognition on real recordings

`crates/naghma-audio/examples/eval_maqam.rs` measures how well the rule-based
`MaqamAnalyzer` recognises maqamat in labelled recordings. Every clip goes
through the full pipeline (YIN → note tracker), then its completed notes are
replayed into the analyser the way a live application feeds it, with an
analysis every `--interval` seconds.

```text
cargo run --release -p naghma-audio --example eval_maqam -- path/to/manifest.json \
    [--interval 2] [--window 30] [--tolerance 30] [--config '{"minScore":0.5}'] \
    [--tracker '{"minConfidence":0.8}'] [--kind taqsim,doulab] [--only id1,id2] \
    [--verbose] [--evidence] [--json report.json]
```

`--config` takes a JSON object of `AnalyzerConfig` fields (camelCase) merged
over the defaults, so each scoring hypothesis can be switched on or off
without recompiling.

## Metrics

| Column | Meaning |
| --- | --- |
| `top1` / `top3` | the stated maqam is the best / among the best three candidates at the end of the clip |
| `step1` / `step3` | the same, over every analysis that had enough data: how often a live display would be right |
| `tonic` | the candidate for the stated maqam has the stated tonic (pitch class), for clips whose source states one |
| `status` | final status; the aggregate reports the status share over all analyses |
| `offset` | duration-weighted circular mean deviation of the notes from the 24-EDO grid, cents (independent of the analyser) |
| `first` | seconds until the best candidate was first correct |

`step1` is the most robust number: a 60-second clip yields about 30 analyses,
while `top1` is a single reading at an arbitrary point in a performance.

## Manifest format

See [`evaluation/manifest.example.json`](evaluation/manifest.example.json).
Only `id`, `path`, `maqam`, `tonic`, `kind` and `include` are read; other
fields record provenance.

* `path` — mono or multi-channel WAV, relative to the manifest or absolute.
* `maqam` — a dataset id (`saba`, `nahawand`, `ajam`, `bayati`, `sikah`,
  `hijaz`, `rast`, `kurd`), or `null` for non-maqam material (skipped).
* `tonic` — `{"degree": "re", "accidental": 0}` (accidental in quarter-tones),
  only when the source states it.
* `include: false` — keep a clip documented but out of the aggregate (for
  example when the description says it mixes several maqamat).

**Labels must come from the source** (title, description, on-screen text),
never from this analyser's output. Recordings are not distributed with the
repository.
