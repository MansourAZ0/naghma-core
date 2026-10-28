# Eight maqamat: source record and engineering interpretation

Research date: 2026-10-22. Dataset:
`crates/naghma-core/data/maqamat/eight-24edo.json`.

The mnemonic «صنع بسحرك» identifies Saba, Nahawand, Ajam, Bayati, Sikah,
Hijaz, Rast and Kurd. It is an introductory grouping, not a complete catalogue
of Arabic maqamat. Research consulted MaqamWorld's explanations and visually
checked all eight notation diagrams, then independently cross-checked the
transposition formulas against Sami Asmar's teaching chart.

The JSON contains factual interval/structural data with independently written
summaries. Source notation images, recordings and page prose are not bundled.
Each maqam, variant and jins has source URLs. The core remains framework-free.

## Source-specific reference tonics and alternatives

| Maqam | Reference tonic in MaqamWorld's illustration | Root / subsequent ajnas (degree numbers are 1-based) |
| --- | --- | --- |
| [Saba](https://www.maqamworld.com/en/maqam/saba.php) | Re | Saba; overlapping Hijaz on 3; Nikriz **or** three-note Ajam on 6 |
| [Nahawand](https://www.maqamworld.com/en/maqam/nahawand.php) | Do | Nahawand; Hijaz **or** Kurd on 5 |
| [Ajam](https://www.maqamworld.com/en/maqam/ajam.php) | Do | Five-note Ajam; Upper Ajam **or** Nahawand on 5 |
| [Bayati](https://www.maqamworld.com/en/maqam/bayati.php) | Re | Bayati; Nahawand **or** Rast on 4 |
| [Sikah](https://www.maqamworld.com/en/maqam/sikah.php) | Mi half-flat | Sikah; Upper Rast on 3 (its tonic is degree 6); Rast on 6. Alternative: the [Huzam](https://www.maqamworld.com/en/maqam/huzam.php) route, Sikah; Hijaz on 3; Rast on 6 |
| [Hijaz](https://www.maqamworld.com/en/maqam/hijaz.php) | Re | Hijaz; Nahawand **or** Rast on 4 |
| [Rast](https://www.maqamworld.com/en/maqam/rast.php) | Do | Rast; Upper Rast **or** Nahawand on 5 |
| [Kurd](https://www.maqamworld.com/en/maqam/kurd.php) | Re | Kurd; Nahawand on 4 |

These reference tonics are not restrictions on transposition. The companion
allows every quarter-tone tonic. Transposing a formula does not automatically
create a traditionally named independent maqam.

### Bayati examples

The Nahawand upper route, using fixed-do solfege:

- Re: Re · Mi half-flat · Fa · Sol · La · Si flat · Do · Re.
- Sol: Sol · La half-flat · Si flat · Do · Re · Mi flat · Fa · Sol.
- La: La · Si half-flat · Do · Re · Mi · Fa · Sol · La.

For the Rast upper route, raise the sixth of each sequence by a notated
quarter-tone: Si half-flat, Mi half-flat, and **Fa half-sharp**, respectively.
[Sami Asmar's chart](https://www.turath.org/Resources/MaqamTrans.htm) uses this
Rast alternative in the main Bayati rows and marks the other pitch in parentheses.

### Three important distinctions

1. [Upper Rast](https://www.maqamworld.com/en/jins/upper_rast.php) and
   [Upper Ajam](https://www.maqamworld.com/en/jins/upper_ajam.php) have their
   tonic at the top of a four-note span. They are not ordinary root-position
   five-note Rast/Ajam moved upward. `tonicStep` records this distinction.
2. Saba's Nikriz route includes Re-flat above the reference Re, then Mi-natural
   above a lower Mi-half-flat. The Ajam alternative instead reaches Re-natural.
   Neither branch is indiscriminately octave-repeated. The dataset's Saba
   teaching targets are limited to the documented register relative to the
   selected tonic octave. Saba's root jins has ambiguous size; its six-degree
   illustrated span is not asserted to be a universal fixed-size jins.
3. Egyptian Ajam in the reference is on Do. [Ajam Ushayran](https://www.maqamworld.com/en/maqam/ajam_ushayran.php)
   is not just a label for any mathematical Ajam transposition onto Si-flat:
   the source describes a different sayr/structure. Likewise, Muhayyar is
   distinguished by an octave-led sayr. No unsourced aliases are generated.

## Intonation and recognition limits

All numeric targets are an **explicit 24-EDO conversion of written notation**.
A half-flat is represented by 50 cents below the corresponding natural note.
These are not measured or universally correct Arabic performance intervals.
For example, [Jins Hijaz](https://www.maqamworld.com/en/jins/hijaz.php) describes
raising the written second and lowering the third slightly in performance.
No exact replacement cents values are invented.

The data is therefore `provisional`, pending review by the project's musician.
No characteristic phrases or modulation paths are invented. `restingSteps`
records the source's structural ghammaz degrees, not a complete account of sayr.
Direction-dependent examples are offered as alternatives; the engine does not
claim to recognize melodic direction or an entire maqam from a scale alone.

Alternative formulas are ranked independently, with the selected formula named
in the result. They are not unioned into one permissive scale. A real performance
may use several valid routes, so distance from one illustration does not establish
that a note is foreign to the maqam. Saba scoring uses
absolute register-aware distances and reports the inferred tonic octave.
Other illustrated formulas use pitch classes for the existing heuristic scorer.

Regression tests check the source reference spellings, Bayati on Re/Sol/La,
all 24 tonic transpositions, separate upper-jins routes, Upper Rast/Ajam tonic
placement and Saba's nonperiodic behavior. These are structural and synthetic
recognition tests. Recognition has since been measured against 85 labelled
public oud recordings ([EVALUATION.md](EVALUATION.md)); musician review of the
data remains necessary.

Single recordings depart from these conversions (the Sikah scale demo puts
its tonic about 14 cents below Mi natural rather than 50). Measured over all
85 recordings, though, most degrees sit within about 10 cents of the 24-EDO
values on average, with a wide spread between players
([EVALUATION.md](EVALUATION.md#performed-intonation-unreleased)). The
analyser absorbs the spread algorithmically — soft membership with an
adjustable tolerance and global tuning-offset compensation — without
changing the dataset's values.

Sikah has a second illustration, the Huzam route (Hijaz on the 3rd degree).
MaqamWorld calls Huzam "by far the most common maqam in the Sikah family",
and recordings labelled Sikah often follow it. It is an alternative of the
Sikah entry, like the upper-jins routes of other maqamat, not a ninth
maqam.

## Schema compatibility

Schema 2 adds per-entry sources, upper-jins tonic placement, alternatives and
explicit octave-periodicity. The new loader still accepts legacy schema-1 JSON.
Schema 1 cannot declare the new musical semantics, so older applications reject
schema 2 rather than silently octave-folding Saba. The Rust public struct changes
are released under the explicit 0.4.0 minor version.
