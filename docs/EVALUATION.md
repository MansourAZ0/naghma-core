# Evaluating maqam recognition on real recordings

`crates/naghma-audio/examples/eval_maqam.rs` measures how well the rule-based
`MaqamAnalyzer` recognises maqamat in labelled recordings. Every clip goes
through the full pipeline (YIN → note tracker), then its completed notes are
replayed into the analyser the way a live application feeds it, with an
analysis every `--interval` seconds.

```text
cargo run --release -p naghma-audio --example eval_maqam -- path/to/manifest.json \
    [--interval 2] [--window 10] [--tolerance 30] [--config '{"minScore":0.5}'] \
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

## Corpus (October 2026)

Public YouTube recordings of solo oud whose **title or description names the
maqam**; tonics are recorded only where the source states one (6 clips). Audio
only, converted to mono 48 kHz; one segment per recording (the first ~60 s
after the first sound; full length for short scale demos and doulabs). The
audio is **not** in this repository; the source list below allows the corpus
to be rebuilt.

* **Development set** — 54 labelled clips: 8 single-line scale demonstrations
  (one per maqam, Taqasim School), 6 short doulabs, 40 taqasim; 6–8 per
  maqam. Used for every measurement and parameter choice below. Two
  open-string tuning references were used only to check frequencies.
* **Held-out set** — 31 further clips (4 per maqam, 3 for Hijaz), mostly
  different players, downloaded **before** any tuning and evaluated only once
  the parameters were fixed. One keyboard (not oud) Ajam clip is included.
* Excluded from aggregates (`include: false`): a Sikah video whose description
  lists several sikah-family maqamat, and a Hijaz video whose description says
  it mixes Hijaz with Rast, Saba and Bayati.

### Frequency checks on the easy material

* Scale demos read as stated: e.g. the Rast demo gives Do3 Re3 Mi half-flat Fa
  Sol La Si half-flat Do4 (Si flat descending), every note within ±15 cents of
  24-EDO; the Bayati demo's oud is +8 c in the low register and up to +23 c
  higher up. Open-string references match their stated strings.
* Instruments are rarely at La4 = 440 Hz: grid offsets span −22…+23 c.
* Performed intonation differs from the 24-EDO conversion: half-flats measured
  from −60 c (Rast demo, Mi half-flat) to about −14 c (the Sikah demo's tonic)
  below the natural; Saba's second degree is often played close to the flat.
* Pluck transients: the first ~85 ms of a note can read up to ~35 c flat and
  form a separate short note; open low courses ring and octave-flip (Re2/Re3),
  which is harmless for pitch-class analysis.
* Tracker bug found and fixed: an attack could be used as the start of two
  notes, producing overlapping events (9 per minute in one taqsim).

## Results: v0.4.0 → v0.5.0

`step1` = share of analyses (every 2 s, rolling window) whose best candidate is
the stated maqam; `top1` = best candidate at the end of the clip; `step3` /
`top3` = among the best three.

| Set | Version | step1 | step3 | top1 | top3 | stated tonic |
| --- | --- | --- | --- | --- | --- | --- |
| Development (54) | 0.4.0 | 49% | 82% | 57% | 81% | 2/5 |
| Development (54) | 0.5.0 | **71%** | **90%** | **72%** | **93%** | 3/5 |
| Held-out (31) | 0.4.0 | 44% | 77% | 45% | 68% | 2/3 |
| Held-out (31) | 0.5.0 | **63%** | **88%** | **55%** | **87%** | 3/3 |

Per maqam, `step1` (development / held-out):

| Maqam | 0.4.0 dev | 0.5.0 dev | 0.4.0 held-out | 0.5.0 held-out |
| --- | --- | --- | --- | --- |
| Ajam | 75% | 93% | 72% | 77% |
| Bayati | 55% | 61% | 46% | 58% |
| Hijaz | 46% | 79% | 64% | 85% |
| Kurd | 49% | 70% | 46% | 73% |
| Nahawand | 52% | 70% | 61% | 82% |
| Rast | 50% | 78% | 48% | 72% |
| Saba | 26% | 66% | 11% | 52% |
| Sikah | 41% | 44% | 7% | 15% |

The single-line scale demos went from 56–88% to 94–100% of analyses correct
for seven maqamat; the Sikah demo stays wrong (see limits). The "stated tonic"
misses are consistent with transposed instruments (an "Ajam on Fa / Jiharkah"
recording sounds a semitone lower, a common oud tuning); applications should
use the player's transposition setting.

### Hypotheses measured on the development set

Each row changes one thing relative to the final configuration (`step1` /
`top1`). The baseline for this table was the configuration at the time of the
ablation, with a 0.08 s minimum note: 70% / 72%.

| Change | step1 | top1 | Kept? |
| --- | --- | --- | --- |
| Hard 30-cent cutoff instead of soft membership | 63% | 63% | soft kept |
| No octave comparison outside Saba's documented register | 69% (Saba 75% → 24%) | 70% | folding kept |
| No tuning-offset compensation | 71% | 69% | kept (see note) |
| Offset search bound ±10 c / ±25 c instead of ±20 c | 69% / 69% | 70% / 70% | ±20 c |
| Ignore offsets below 8 c | 70% | 72% | no |
| Extra +10 c / +20 c tolerance on quarter-tone degrees | 71% / 67% | 72% / 72% | no |
| Minimum note 0 s / 0.15 s (vs 0.08 s) | 70% / 71% | 69% / 74% | 0.15 s |
| Tolerance 20 / 25 / 35 c (vs 30 c) | 66% / 68% / 69% | 70% / 70% / 70% | 30 c |
| Window 20 / 25 / 40 s (vs 30 s) | 64% / 68% / 72% | 65% / 63% / 70% | 30 s (app requirement) |
| Cadence term (notes followed by ≥ 0.25 s pause, weight 0.2 / 0.4) | −3…−6 pts | −2…−7 pts | no |
| "Latest long note on tonic" weight 0.1 / 0.2 (vs 0) | 67% / 63% | 81% / 78% | 0 (noisy) |

Tuning-offset compensation is roughly neutral on aggregate because most
instruments here are within ±10 c, but it decides the clearly detuned ones
(Hijaz at +18 c: 3% → 41% of analyses; Rast at +14 c: 81% → 96%) and is the
reason a detuned instrument keeps the correct tonic name. It hurt one old
commercial Sikah recording (+15 c) where the performed intonation itself is
far from the 24-EDO grid. The cumulative effect of the changes is far larger
than any single row because they interact (soft membership alone was neutral
before the tonic weighting changed).

## Performed intonation (0.6.0)

`crates/naghma-audio/examples/measure_intonation.rs` measures where players
put each degree. Per clip it removes the tuning offset, picks the tonic under
which the stated maqam fits best (all 24 classes, 45 c tolerance), assigns
each note to a scale degree when it is clearly nearest to one, and records
the deviation from the 24-EDO illustration, weighted by duration.

Development and held-out sets measured separately (weighted median
deviation in cents; degrees heard in at least 3 clips of both sets):

| Degree | Development | Held-out |
| --- | --- | --- |
| Bayati 2nd (Mi half-flat on Re) | −12 | −12 |
| Bayati 5th (La on Re) | +8 | +16 |
| Nahawand raised 7th (Si on Do) | +11 | +12 |
| Rast 3rd (Mi half-flat on Do) | −4 | −4 |
| Rast 7th (Si half-flat on Do) | −4 | −3 |
| Saba 4th (Sol flat on Re) | −13 | +3 |
| Sikah tonic (Mi half-flat) | −6 | +10 |
| Sikah 5th (Si half-flat on Mi half-flat) | +12 | +30 |

Most other degrees are within ±10 c in both sets; interquartile ranges are
typically 10–30 c wide, wider for quarter-tones. Only a few shifts repeat
in both sets (Bayati's 2nd about 12 c low, Nahawand's raised 7th about 11 c
high), and a 30 c tolerance already covers them.

**Measured positions do not improve recognition, so the dataset keeps its
24-EDO values.** Scoring with per-degree positions measured on one set and
evaluated on the other: held-out 49% → 49% of analyses (top-3 78% → 80%),
development 54% → 56%. A musician should review the shifts before any
application presents them as targets.

The held-out Sikah recordings lower their 4th degree toward La flat (median
−43 c from La) and raise their 5th toward Si natural (+30 c from Si
half-flat): the direction of Hijaz on the 3rd degree, the scale of Maqam
Huzam.
Adding that route as a Sikah alternative (sourced from MaqamWorld, not
fitted to the data) raised Sikah from 28% to 43% (development) and 23% to
50% (held-out) of analyses, and the totals to 55% and 52%.

## Note-tracking artefacts on real recordings (0.6.0)

`crates/naghma-audio/examples/tracker_artifacts.rs` counts patterns that are
errors whatever the music is, so it needs no note labels: a pluck's
transient reported as its own note (**split**: a plucked note under 150 ms
followed at once, with no new pluck, by one less than a semitone away), and
a harmonic taken for the fundamental (**octave**: a jump of an octave, a
twelfth or two octaves with no new pluck, which a fretted oud cannot make).
It also reports how long the screen shows a name the note does not end up
with, and the time from an attack to the note being shown.

```text
cargo run --release -p naghma-audio --example tracker_artifacts -- manifest.json \
    [--tracker '{"settleS":0,"harmonicGuard":false}']
```

All 89 clips of both sets (83 minutes, including the two open-string
tuning references):

| Tracker | Notes/min | Splits/min | Octave flips/min | Wrong name shown | Attack → shown, median (p90) |
| --- | --- | --- | --- | --- | --- |
| 0.5.0 | 223 | 9.1 | 26.7 | — | 85 ms (171 ms) |
| Harmonic guard only | 202 | 8.9 | 8.4 | — | 85 ms (171 ms) |
| Guard + settle 0.08 s | 195 | 4.5 | 6.8 | 0.8% | 85 ms (171 ms) |
| **Guard + settle 0.12 s** | 188 | **0.8** | **5.4** | 2.6% | 85 ms (171 ms) |
| Guard + settle 0.16 s | 183 | 0.2 | 4.8 | 4.6% | 85 ms (171 ms) |

"Wrong name shown" is the pluck's first reading on screen before the note
is renamed; before this change the same reading was a separate short note
(counted as a split). 0.12 s covers the measured ~85 ms transient plus the
analysis window's lag. Maqam recognition with the new tracker (10 s window):
development 54% → 54%, held-out 46% → 49% of analyses.

**Fast attack reading.** A full 85 ms window must be mostly past a pluck
before it reads the new note. Until a plucked note is shown, the analyser
now reads it from the newest 43 ms (`fastAttack`), and the full window then
confirms the name or the tracker renames the note while it settles. On the
same 89 clips the median attack → shown time fell from **85 ms to 64 ms**
(p90 171 → 160 ms); splits (0.84 → 0.92/min) and octave flips (5.4 → 5.3/min)
were unchanged, the wrong name was shown 3.6% instead of 2.6% of the time,
and maqam recognition was unchanged (55% / 51% of analyses). A 2048-sample
window throughout would reach 64 ms too but triples the artefacts (splits
7.5/min, octave flips 14.3/min). On clean synthetic plucks both paths show
the note 48–56 ms after the attack.

Limits: an octave error that lasts a whole note from its attack (the
detector reading the second harmonic of a pluck throughout) is not caught;
the remaining "octave flips" include real octave leaps whose pluck the
attack detector missed. Latency is algorithmic only (no device buffering).

## Ten-second window (0.6.0)

The maqam window was shortened from 30 s to 10 s at a player's request: in a
taqsim, 30 s mixes passages in different maqamat that share most notes. Two
changes recover part of the evidence a short window loses. The tuning offset
is estimated over the last 60 s (it belongs to the instrument, not to the
passage), and the maqam and tonic that led the previous analysis get a small
bonus (`continuity`, 0.1), so a near-tie does not flip the display.

| Set | 30 s (0.5.0) | 10 s plain | 10 s + tuning context + continuity |
| --- | --- | --- | --- |
| Development (54), step1 / step3 | 71% / 90% | 50% / 81% | **54% / 82%** |
| Held-out (31), step1 / step3 | 63% / 88% | 44% / 79% | **46% / 79%** |
| Development scale demos and doulabs (14), step1 | 75% | — | 74% |
| Development taqasim (40), step1 | 70% | — | 51% |

Continuity weights 0 / 0.03 / 0.06 / 0.1 / 0.15 gave development step1 50% /
52% / 53% / 54% / 54%; weights for coverage, tonic, final rest, minimum note
length and tolerance moved it by at most two points at 10 s.

**These numbers understate a short window.** Every label covers a whole
clip. When a taqsim moves to another maqam for a while, a 10 s window that
follows it is counted wrong. Material that stays in one maqam (scale demos,
doulabs) scores the same at 10 s as at 30 s, and with 10 s every clip was
named correctly at some point (30 s: 3 never were). A fair measurement needs
time-stamped labels from a musician. Maqamat that share a pitch set on a
related tonic (Sikah/Bayati/Rast, Hijaz/Nahawand/Kurd) are the main
confusions, as before; ten seconds simply holds fewer notes to separate them.

## Jins reading and score memory (0.7.0)

A taqsim dwells in a maqam's upper jins for long stretches. With a 10 s
window and no history, a passage in Hijaz's upper Nahawand on Sol was ranked
maqam Nahawand on Sol: Hijaz on Re (Hijaz on Re + Nahawand on Sol) and
Nahawand on Sol (Nahawand on Sol + Hijaz on Re) have exactly the same notes.
0.7.0 adds:

* **Score memory.** Every maqam and tonic keeps a decaying score history,
  blended half-and-half with the window's score. History falls slowly
  (60 s) while the notes stay in the maqam's scale, faster in proportion to
  notes outside it, and is dropped once a jins outside the maqam is
  confirmed. Rising evidence is followed within 10 s, but only while the
  leading maqam is contradicted (≥ 10% foreign notes, then for 20 s), so the
  same-notes rotation cannot creep in during an upper-jins passage.
* **Cadence** (0.2): share of phrase endings (long tones, tones before a
  pause, tremolo merged into one tone) on the tonic. **Register** (0.3):
  melody from a little below the tonic to a little above its octave.
  **Lower jins** (0.2): completeness of the jins on the tonic.
* **Active jins** for the latest 5 s, as a placement of the leading maqam
  (first jins or an upper one), and a **foreign jins** (possible modulation)
  when ≥ 10% of that passage lies outside the maqam's scale, a jins explains
  ≥ 70% of those notes, it is not one of the maqam's documented routes, and
  it is heard in two analyses in a row.

| Set | Measure | 0.6.0 | 0.7.0 |
| --- | --- | --- | --- |
| Development (54) | step1 / step3 | 55% / 83% | **70% / 91%** |
| Development (54) | top1 / top3 | 56% / 85% | **70% / 89%** |
| Held-out (31) | step1 / step3 | 52% / 81% | **73% / 91%** |
| Held-out (31) | top1 / top3 | 32% / 71% | **55% / 77%** |
| Development / held-out | ambiguous share | 24% / 22% | 13% / 11% |
| Both | analyses flagging a possible modulation | — | 1–2% (scale demos 1%) |

Per maqam, `step1` (development / held-out): Ajam 71→91% / 65→80%, Bayati
45→60% / 55→76%, Hijaz 53→70% / 68→82%, Kurd 56→61% / 51→83%, Nahawand
45→71% / 61→87%, Rast 58→86% / 37→71%, Saba 61→66% / 36→49%, Sikah
44→52% / 49→57%. (The 0.6.0 column is the 0.6.0 release; the ten-second table above
was measured before that release's note-tracking fixes.)

Development ablations (`step1` / `top1`), each from the configuration at the
time of the ablation (68% / 74%, before the modulation responsiveness below):
no memory 55% / 61%; no cadence 70% / 81%; no register 68% / 74%; no lower
jins 68% / 81%; memory only 68% / 78%. **Memory gives almost all of the
gain on these recordings.** Cadence, register and lower jins are within
noise on the corpus, but decide the rotation case it barely contains: on one
Hijaz phrase through both ajnas and without history, the Hijaz-over-Nahawand
margin is 0.06 (ambiguous) with neither, 0.14 with cadence, 0.10 with
register and 0.18 with both. A grid of 64 weight combinations on the
development set spanned 63–71% step1; the chosen weights are in its flat
top, not its single best cell.

**Responsiveness costs accuracy on whole-clip labels.** On synthetic Bayati
on Re followed by Hijaz on Re, the possible modulation is flagged after 5 s
and the leading maqam changes after 14 s (no memory: 6 s; memory without the
contradiction rule: 25 s). Faster rise times (5 s → 9 s to switch) cost two
points of development step1, slower ones (20 s → 20 s) gain one: the labels
ignore modulation and reward a reading that never moves. The two remaining
Hijaz → Nahawand end-of-clip confusions are passages resting on Sol, 20 s and
more, with 70–87% of phrase endings on it: plausibly the common Hijaz
modulation to Nahawand on its fourth, which only time-stamped labels can
settle. A fixed tonic (qarar) setting removes the rotation entirely.

## The performance as a whole (0.8.0)

Every label in the corpus names the maqam of a whole piece, and that is
also what a player asks first: which maqam is this taqsim in? A ten-second
window answers a different question (what is being played now) and follows
every passage into a related maqam. `MaqamAnalysis::performance` ranks the
maqamat over everything played since the piece began, up to
`performanceWindowS` (300 s), with the same evidence terms and no score
memory. A silence of `performanceGapS` (20 s) between notes starts a new
piece. The ten-second reading, the active jins and the modulation flag are
unchanged.

The 60-second corpus cannot show a longer horizon, so the same recordings
were also evaluated at full length (`full.wav` beside each segment:
165 minutes development, 136 minutes held-out), labels unchanged.

| Set | Reading | top1 | step1 |
| --- | --- | --- | --- |
| Development, 60 s clips (54) | ten-second window | 70% | 70% |
| Development, 60 s clips (54) | performance | **87%** | **79%** |
| Held-out, 60 s clips (31) | ten-second window | 55% | 73% |
| Held-out, 60 s clips (31) | performance | **90%** | **81%** |
| Development, full recordings (54) | ten-second window | 57% | 63% |
| Development, full recordings (54) | performance | **91%** | **87%** |
| Held-out, full recordings (31) | ten-second window | 55% | 55% |
| Held-out, full recordings (31) | performance | **84%** | **72%** |

The performance reading is `likely` in 70% / 58% of its readings on the
full recordings (development / held-out) and then right in 96% / 89%.

Nothing was fitted for it. On the development full recordings, horizons of
120 / 300 / 600 s / unbounded gave step1 79% / 87% / 87% / 87%; gaps of 0,
10, 20 and 40 s were identical; and fifteen single-weight changes (evidence
weights lowered, raised or removed, one at a time) moved step1 between 84%
and 88%, except adding the final-rest term at 0.2 (79%). It costs about 5 ms per analysis at 300 s on an Apple M2.

Found on the way: `ScoreWeights` read a missing `memory`, `continuity`,
`cadence`, `register` or `lowerJins` as 0 when only some weights were given
in JSON. They now keep their defaults. Ablations that passed a partial
`weights` object to `--config` before this fix switched those five terms
off together.

`crates/naghma-audio/examples/dump_notes.rs` prints the notes tracked in a
recording, or with `--frames` a per-second count of voiced, aperiodic and
silent frames. It showed that the 33 seconds without notes at the start of
one 1955 recording are steady, aperiodic sound at −22 dB (not music the
detector misses).

## Limits

* Labels are whatever the uploader wrote. Taqasim modulate; a 60-second
  segment of a "Rast" taqsim can legitimately sit in another maqam for a while,
  so 100% is not the ceiling for `step1`.
* **Sikah is the weakest maqam** (43% / 50% of analyses with the Huzam
  route, 28% / 23% without, at 10 s). The single Sikah scale demo places its
  tonic about 14 c below Mi rather than 50, but across all Sikah recordings
  the tonic's placement is inconsistent (see Performed intonation).
* Most remaining errors are tonic choices between maqamat that share a pitch
  set (Rast/Bayati/Sikah, Kurd/Nahawand/Hijaz on related tonics); 0.7.0's
  score memory reports fewer of them as `ambiguous` (13% / 11% of analyses,
  from 24% / 22%), so a wrong early reading can now persist until the notes
  contradict it.
* The active jins is read from pitch classes over 5 s. It names the
  placement that fits best; it does not yet use register to tell a jins
  played below the tonic from the same jins an octave higher.
* Small corpus (85 clips, 6 with stated tonics), one segment per recording, and
  the development set was used for parameter choices; the held-out numbers are
  the fairer estimate.

## Reproducing

1. Download each listed source's audio, convert to mono 48 kHz WAV
   (`ffmpeg -i src -ac 1 -ar 48000 full.wav`) and cut the listed segment.
2. Write a manifest (format above) pointing at the segments.
3. `cargo run --release -p naghma-audio --example eval_maqam -- manifest.json`.

### Development set (used for tuning)

| id | stated maqam | stated tonic | kind | used | source |
| --- | --- | --- | --- | --- | --- |
| scale-rast-ts | rast |  | scale | yes | [Oud Beginners - Maqam Rast Scale](https://www.youtube.com/watch?v=zKQz8E8aB4M) (from 2.2 s, 23 s) |
| scale-bayati-ts | bayati |  | scale | yes | [Oud Beginners - Maqam Bayat Scale](https://www.youtube.com/watch?v=roJmZv032mI) (from 1.9 s, 17 s) |
| scale-ajam-ts | ajam |  | scale | yes | [Oud Beginner - Maqam Ajam Scale](https://www.youtube.com/watch?v=Mp260ZaAn8k) (from 1.6 s, 20 s) |
| scale-saba-ts | saba |  | scale | yes | [Oud Beginners - Maqam Saba Scale](https://www.youtube.com/watch?v=b-KWqLDXRuU) (from 2.3 s, 56 s) |
| scale-kurd-ts | kurd |  | scale | yes | [Oud Beginners - Maqam Kurd Scale](https://www.youtube.com/watch?v=2OWD8kuacxA) (from 2.9 s, 42 s) |
| scale-sikah-ts | sikah |  | scale | yes | [Oud Beginners - Maqam Segah Scale](https://www.youtube.com/watch?v=IP8cWM5iTgQ) (from 2.1 s, 22 s) |
| scale-nahawand-ts | nahawand |  | scale | yes | [Oud Beginners - Maqam Nahawand](https://www.youtube.com/watch?v=2IllTHbv3LI) (from 2.8 s, 40 s) |
| scale-hijaz-ts | hijaz |  | scale | yes | [Oud Beginners - Maqam Hijaz Scale](https://www.youtube.com/watch?v=k74TTCa-RT8) (from 2.1 s, 37 s) |
| rast-ibrahim | rast |  | taqsim | yes | [Taqasim Oud Maqam Rast / تقاسيم عود على مقام راست](https://www.youtube.com/watch?v=m4cLPohrUVY) (from 0.8 s, 60 s) |
| rast-khatib | rast |  | taqsim | yes | [Ahmad Al Khatib, Taqsim Rast. Arabic style, تقسيم راست , احمد الخطيب](https://www.youtube.com/watch?v=M1bCW_7YT9U) (from 0.0 s, 60 s) |
| rast-balays | rast |  | taqsim | yes | [Taqsim oud maqam Rast -  تقسيم قصير على العود : مقام راست](https://www.youtube.com/watch?v=QIBFl2xStOY) (from 0.2 s, 58 s) |
| rast-oved | rast |  | taqsim | yes | [Taqsim In Maqam Rast On Egyptain Oud  תקסים במקאם ראשט עוד מצרי](https://www.youtube.com/watch?v=sniSMPCmTJc) (from 0.0 s, 60 s) |
| rast-bashir | rast |  | taqsim | yes | [Taqsîm En Maqâm Râst](https://www.youtube.com/watch?v=SzzVmVagf4g) (from 0.1 s, 60 s) |
| rast-doulab-ts | rast |  | doulab | yes | [Oud Beginners - Doulab Rast](https://www.youtube.com/watch?v=9vPlIgLgnrU) (from 2.9 s, 63 s) |
| bayati-hamra | bayati |  | taqsim | yes | [Oud Taksim Bayātī (بياتي, Uşşâk, Bayat) by Hesham Hamra (4k Video) 2020](https://www.youtube.com/watch?v=1DrzYlc8DAM) (from 3.7 s, 60 s) |
| bayati-alsalem | bayati |  | taqsim | yes | [تقاسيم عود مقام بياتي Taqasim maqam Bayati](https://www.youtube.com/watch?v=Bg_U8nntsbM) (from 0.9 s, 60 s) |
| bayati-bitar | bayati |  | taqsim | yes | [Taqasim Bayati](https://www.youtube.com/watch?v=-Y9Xl8w9UuM) (from 0.0 s, 60 s) |
| bayati-qasabji | bayati |  | taqsim | yes | [Taqsim Oud Maqam Bayati - Muhamed al-Qasabji  - 1955   القصبجي](https://www.youtube.com/watch?v=OQSmPsX3SIA) (from 0.0 s, 60 s) |
| bayati-jadue | bayati |  | taqsim | yes | [yuseff jadue - taksim oud maqam bayati-](https://www.youtube.com/watch?v=_Tv-WEY61_I) (from 0.3 s, 47 s) |
| hijaz-ibrahim | hijaz |  | taqsim | yes | [Taqasim Oud - Maqam Hijaz](https://www.youtube.com/watch?v=DZ9lwxl37-I) (from 1.2 s, 60 s) |
| hijaz-oved | hijaz |  | taqsim | yes | [תקסים במקאם חיג'אז על עוד Taqsim In Maqam Hijaz On Oud](https://www.youtube.com/watch?v=GvnBrl5FSVs) (from 0.0 s, 60 s) |
| hijaz-holm | hijaz |  | taqsim | yes | [Oud Taqsim (March 14, 2017) - Maqam Hijaz](https://www.youtube.com/watch?v=9Ok-jc3uHl8) (from 10.7 s, 60 s) |
| hijaz-abdullah | hijaz |  | taqsim | yes | [Oud Taqsim Maqam Hijaz](https://www.youtube.com/watch?v=IBdUJcNNzJ4) (from 0.0 s, 60 s) |
| hijaz-abbad | hijaz |  | taqsim | yes | [Omar Abbad - Oud Taqsim, Maqam Hijaz](https://www.youtube.com/watch?v=gnTmBmUhpho) (from 0.0 s, 60 s) |
| nahawand-arkach | nahawand | Do | taqsim | yes | [Takassim Nahawand Do](https://www.youtube.com/watch?v=QpeXnaR7hFg) (from 0.4 s, 60 s) |
| nahawand-khayyat | nahawand |  | taqsim | yes | [Taqasim Oud _ Nahawand  /تقاسيم عود نهاوند  /سيف الخياط](https://www.youtube.com/watch?v=FsEDiZN9GgI) (from 0.0 s, 60 s) |
| nahawand-ashkar | nahawand |  | taqsim | yes | [Yuval Ashkar Nahawand Taqasim](https://www.youtube.com/watch?v=CXA4izJVqIg) (from 0.1 s, 60 s) |
| nahawand-dampwater | nahawand |  | taqsim | yes | [Taqseem Nahawand on Oud](https://www.youtube.com/watch?v=nF2Fi3YdMGA) (from 9.1 s, 60 s) |
| nahawand-khatib | nahawand |  | taqsim | yes | [Taqsim Nahawand](https://www.youtube.com/watch?v=LAsb0u9kg4I) (from 0.0 s, 60 s) |
| nahawand-doulab-ts | nahawand |  | doulab | yes | [Oud Beginners - Doulab Nahawand Qadim](https://www.youtube.com/watch?v=eE1KmhzydGQ) (from 1.6 s, 50 s) |
| kurd-ibrahim | kurd |  | taqsim | yes | [Taqasim Kurd](https://www.youtube.com/watch?v=TdhPGqKH-lk) (from 0.0 s, 60 s) |
| kurd-oved | kurd |  | taqsim | yes | [תקסים במקאם כורד על עוד Taqsim In Maqam Kurd On Oud](https://www.youtube.com/watch?v=S64oRQlis_Y) (from 0.0 s, 60 s) |
| kurd-benyoucef | kurd |  | taqsim | yes | [taqsim oud maqam kurd, Raphaël Benyoucef](https://www.youtube.com/watch?v=i7HsqK1HXWc) (from 0.9 s, 60 s) |
| kurd-taboud | kurd | La | taqsim | yes | [تقاسيم عود على مقام كرد لا](https://www.youtube.com/watch?v=YDWLczLgyes) (from 0.0 s, 60 s) |
| kurd-alsalem | kurd |  | taqsim | yes | [تقاسيم مقام كرد عود .. احساس هادئ Taqasim oud Kord](https://www.youtube.com/watch?v=E90ahRYScWg) (from 0.8 s, 60 s) |
| kurd-esses | kurd |  | taqsim | yes | [Taqsim Oud Maqam kurd ~ Victor Esses ~ New York 2014](https://www.youtube.com/watch?v=p1_SXNl_Nn0) (from 1.0 s, 60 s) |
| kurd-doulab-ts | kurd |  | doulab | yes | [Oud Beginners - Doulab Kurd](https://www.youtube.com/watch?v=ptL5qCG_G6E) (from 2.6 s, 41 s) |
| saba-hamra | saba | Re | taqsim | yes | [Arabic Oud Solo Taksim Re Saba Maqam Hesham Hamra 4k 2020](https://www.youtube.com/watch?v=ERtSDNI4fe4) (from 5.3 s, 60 s) |
| saba-oved | saba |  | taqsim | yes | [תקסים במקאם צבא על עוד Taqsim In Maqam Saba On Oud](https://www.youtube.com/watch?v=Oj94_bv8GQc) (from 0.0 s, 60 s) |
| saba-wach | saba |  | taqsim | yes | [Oud taqsim in maqam "Saba" / تقسيم عود](https://www.youtube.com/watch?v=k5YUJ_i3R30) (from 0.0 s, 60 s) |
| saba-ayash | saba |  | taqsim | yes | [تقاسيم صبا - عود](https://www.youtube.com/watch?v=ngUrXktaTX4) (from 0.0 s, 60 s) |
| saba-oudprof | saba |  | taqsim | yes | [تقاسيم عود مقام الصّبا Saba Oud Taqasim](https://www.youtube.com/watch?v=ze4iflAnFDE) (from 0.0 s, 60 s) |
| saba-doulab-ts | saba |  | doulab | yes | [Oud Beginners - Doulab Saba 1](https://www.youtube.com/watch?v=2UsdWlPYpbI) (from 1.6 s, 46 s) |
| sikah-oved | sikah |  | taqsim | yes | [תקסים במקאם שיגא על עוד Taqsim In Maqam Sikah On Oud](https://www.youtube.com/watch?v=bIrt6MySd8o) (from 0.0 s, 60 s) |
| sikah-arkach | sikah |  | taqsim | yes | [Takassim SIKA](https://www.youtube.com/watch?v=SX9TQTf2WFM) (from 0.0 s, 60 s) |
| sikah-oudprof | sikah |  | taqsim | no | [OUD: SIKAH تقاسيم عود سيكاه](https://www.youtube.com/watch?v=5npzscLqyfk) (from 0.1 s, 60 s) |
| sikah-esses | sikah |  | taqsim | yes | [Taqsim Oud Maqam Sikah ~ Victor Esses ~ New York 2015](https://www.youtube.com/watch?v=4PrHmpNC3Qo) (from 0.1 s, 60 s) |
| sikah-sunbati | sikah |  | taqsim | yes | [Taqaseem sikah](https://www.youtube.com/watch?v=DkMnJlIBa7A) (from 0.1 s, 60 s) |
| sikah-doulab-ts | sikah |  | doulab | yes | [Oud Beginners - Doulab Segah 1](https://www.youtube.com/watch?v=K60JPekkAZ0) (from 2.4 s, 56 s) |
| ajam-oved | ajam |  | taqsim | yes | [תקסים במקאם עג'ם על עוד Taqsim In Maqam Ajam On Oud](https://www.youtube.com/watch?v=a1IMbm_X8A0) (from 0.0 s, 60 s) |
| ajam-arkach | ajam | Do | taqsim | yes | [Takassim AJAM](https://www.youtube.com/watch?v=L7jffYzcL2A) (from 0.7 s, 60 s) |
| ajam-shama | ajam |  | taqsim | yes | [Oud Solo “Ajam” - عود مقام العجم](https://www.youtube.com/watch?v=JUokcjK9LOw) (from 3.0 s, 60 s) |
| ajam-alsalem | ajam | Fa | taqsim | yes | [تقاسيم عود مقام عجم .. مساحة فرح  Taqasim oud Ajam](https://www.youtube.com/watch?v=uwyAgyUOUjQ) (from 0.7 s, 60 s) |
| ajam-oudprof | ajam |  | taqsim | yes | [عود - تقاسيم عجم Oud - Ajam (Major) Imrpov.](https://www.youtube.com/watch?v=imHlXv8S9ag) (from 0.1 s, 60 s) |
| ajam-doulab-ts | ajam |  | doulab | yes | [Oud Beginners - Doulab Ajam](https://www.youtube.com/watch?v=BSceU-WtzMk) (from 1.6 s, 35 s) |

### Held-out set (evaluated only after tuning)

| id | stated maqam | stated tonic | kind | used | source |
| --- | --- | --- | --- | --- | --- |
| ho-rast-qallaf | rast |  | taqsim | yes | [هاشم القلاف - تقاسيم راست](https://www.youtube.com/watch?v=gMjIOYDQiIE) (from 0.0 s, 59 s) |
| ho-rast-hatem | rast |  | taqsim | yes | [عماد حاتم موسيقيون في الظل تقاسيم عود مقام الراست](https://www.youtube.com/watch?v=XISz3i6KKmM) (from 0.0 s, 60 s) |
| ho-rast-sunbati | rast |  | taqsim | yes | [رياض السنباطي - تقاسيم راست](https://www.youtube.com/watch?v=k3CCz6LWzNc) (from 0.5 s, 60 s) |
| ho-rast-shamma | rast |  | taqsim | yes | [naseer shamma --- rast taqsim](https://www.youtube.com/watch?v=5Thvk4h_25g) (from 1.5 s, 60 s) |
| ho-bayati-nasreddine | bayati |  | taqsim | yes | [تقاسيم على مقام البيات](https://www.youtube.com/watch?v=8QJO2Nd-24c) (from 0.5 s, 60 s) |
| ho-bayati-sunbati | bayati |  | taqsim | yes | [Taqaseem bayati](https://www.youtube.com/watch?v=V0uiNETqN3A) (from 0.1 s, 60 s) |
| ho-bayati-bernaoui | bayati |  | taqsim | yes | [تقاسيم بياتي](https://www.youtube.com/watch?v=eW3WqB-i8Vs) (from 0.0 s, 60 s) |
| ho-bayati-muttalib | bayati |  | taqsim | yes | [تقاسيم بياتى من القلب ( سمير عبد المطلب )](https://www.youtube.com/watch?v=pcCgsF3ZO8o) (from 0.1 s, 60 s) |
| ho-hijaz-shama | hijaz |  | taqsim | yes | [Oud solo “Hijaz” - عود مقام حجاز](https://www.youtube.com/watch?v=GigDh-d7wS0) (from 0.0 s, 60 s) |
| ho-hijaz-hamra | hijaz |  | taqsim | yes | [عزف عود ـ هشام حمرا تقاسيم على مقام الحجاز](https://www.youtube.com/watch?v=kfFPdpaPC9Q) (from 0.0 s, 60 s) |
| ho-hijaz-alsalem | hijaz |  | taqsim | no | [حجازيات تقاسيم عود مقام حجاز](https://www.youtube.com/watch?v=DoEEFZaCewU) (from 0.1 s, 60 s) |
| ho-hijaz-sunbati | hijaz |  | taqsim | yes | [رياض السنباطي - تقاسيم حجاز](https://www.youtube.com/watch?v=3nmnOXrTJDY) (from 0.5 s, 60 s) |
| ho-nahawand-sunbati | nahawand |  | taqsim | yes | [Taqaseem nihawand](https://www.youtube.com/watch?v=RzldNCWPkEw) (from 2.7 s, 60 s) |
| ho-nahawand-hamra | nahawand |  | taqsim | yes | [رحلة على العود - عزف عود - تقاسيم على مقام النهاوند هشام حمرا](https://www.youtube.com/watch?v=yTr1UkJoLQY) (from 2.5 s, 60 s) |
| ho-nahawand-nabeel | nahawand |  | taqsim | yes | [تقاسيم نهاوند على عود فاروق تورنز](https://www.youtube.com/watch?v=qHwMe5zsbkA) (from 0.1 s, 60 s) |
| ho-nahawand-bouresli | nahawand | Do | taqsim | yes | [Taqasim Oud C minor تقاسيم مقام النهاوند](https://www.youtube.com/watch?v=wug7KsLgkrM) (from 0.0 s, 60 s) |
| ho-kurd-shoublak | kurd |  | taqsim | yes | [Oud improvisation..Maqam Kurd](https://www.youtube.com/watch?v=9H1CXHDPjkY) (from 1.4 s, 60 s) |
| ho-kurd-badrieh | kurd |  | taqsim | yes | [Oud improvisation Maqam Kurd تقاسيم عود على مقام كرد](https://www.youtube.com/watch?v=HF6kH9csiWo) (from 2.5 s, 60 s) |
| ho-kurd-tidhar | kurd | La | taqsim | yes | [Makam kurd la Takasim](https://www.youtube.com/watch?v=uZKM1Gl8JcY) (from 2.9 s, 60 s) |
| ho-kurd-ararah | kurd |  | taqsim | yes | [تقاسيم مقام كرد](https://www.youtube.com/watch?v=-0vb0QQMbeY) (from 0.1 s, 60 s) |
| ho-saba-khayyat | saba |  | taqsim | yes | [Taqasim Oud Maqam Alsaba / تقاسيم عود  مقام الصبا](https://www.youtube.com/watch?v=KTj87sjQIAc) (from 3.5 s, 60 s) |
| ho-saba-baramawy | saba |  | taqsim | yes | [Yasmine El Baramawy - Oud Improvisation on Saba Maqam - ياسمين البرماوي - ارتجال عود على مقام الصبا](https://www.youtube.com/watch?v=F_i628CM3NQ) (from 0.4 s, 60 s) |
| ho-saba-shehadeh | saba |  | taqsim | yes | [Taqasim  Oud  From  Maqam  Saba  By  NS](https://www.youtube.com/watch?v=AvZ0KVnCdhQ) (from 3.6 s, 60 s) |
| ho-saba-kaabour | saba |  | taqsim | yes | [Taqseem 3 Maqam Saba](https://www.youtube.com/watch?v=gTe7WgayjNk) (from 0.0 s, 60 s) |
| ho-sikah-arkach2 | sikah | Mi half-flat | taqsim | yes | [Takassim SIKA](https://www.youtube.com/watch?v=Gfd2qWN1z1w) (from 0.8 s, 60 s) |
| ho-sikah-oudprof5 | sikah |  | taqsim | yes | [تقاسيم سيكاه على العود Sikah Oud Taqasim/Improvisation 5](https://www.youtube.com/watch?v=euCLrtTM-lA) (from 0.0 s, 60 s) |
| ho-sikah-ouekdi | sikah |  | taqsim | yes | [Taqsim Oud sikah Si Moez EL OUEKDI](https://www.youtube.com/watch?v=346fB5J35NM) (from 0.2 s, 60 s) |
| ho-sikah-chehab | sikah |  | taqsim | yes | [Adel Abed Chehab Doulab and Taksim Maqam Sikah "Talmaciu's Garden" تقاسيم عود](https://www.youtube.com/watch?v=rKlgjs0UkC4) (from 3.8 s, 60 s) |
| ho-ajam-thamer | ajam |  | taqsim | yes | [تقاسيم على مقام العجم بالعود ( عود ثامر)](https://www.youtube.com/watch?v=HSXBqDJyNDg) (from 0.0 s, 60 s) |
| ho-ajam-jneid | ajam |  | keyboard | yes | [تقاسيم مقام عجم](https://www.youtube.com/watch?v=HfWFSr07bW4) (from 0.0 s, 60 s) |
| ho-ajam-bashir | ajam |  | taqsim | yes | [Taqsim in Maqam 'Ajam](https://www.youtube.com/watch?v=Srb91cENvZ4) (from 1.9 s, 60 s) |
| ho-ajam-shaheen | ajam |  | taqsim | yes | [Oud Taqasim Ajam تقاسيم عود عجم](https://www.youtube.com/watch?v=9ISXMG8ZxUA) (from 1.6 s, 60 s) |
