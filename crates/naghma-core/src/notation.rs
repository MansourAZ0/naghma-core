//! Fixed-Do solfège notation with quarter-tone accidentals.
//!
//! Notes are named with the seven fixed-Do degrees — Do, Re, Mi, Fa, Sol,
//! La, Si (دو، ري، مي، فا، صول، لا، سي) — an [`Accidental`] measured in
//! quarter-tones, and an octave number. Octave numbering is scientific: Do4
//! is the Do nearest 261.63 Hz, and the octave number changes between Si and
//! Do.
//!
//! The octave number belongs to the *spelled degree*, not to the sounding
//! frequency: "Do half-flat 4" sounds below Do4 (it is the same pitch as
//! "Si half-sharp 3") but is still written in octave 4.
//!
//! Western letter names (C, D, E, …) are deliberately not provided.
//!
//! ## Provisional terminology
//!
//! The Arabic accidental words and the English words for 3-quarter-tone
//! accidentals used by [`Accidental::arabic_name`] and
//! [`Accidental::english_name`] are common usage but have **not yet been
//! confirmed** for this project; treat them as provisional.

use crate::pitch::Pitch;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// One of the seven natural scale degrees in fixed-Do solfège.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Degree {
    /// Do — دو
    Do,
    /// Re — ري
    Re,
    /// Mi — مي
    Mi,
    /// Fa — فا
    Fa,
    /// Sol — صول
    Sol,
    /// La — لا
    La,
    /// Si — سي
    Si,
}

impl Degree {
    /// All degrees in ascending order from Do.
    pub const ALL: [Degree; 7] = [
        Degree::Do,
        Degree::Re,
        Degree::Mi,
        Degree::Fa,
        Degree::Sol,
        Degree::La,
        Degree::Si,
    ];

    /// Position in the scale, Do = 0 … Si = 6.
    pub fn index(self) -> u8 {
        self as u8
    }

    /// Degree at position `i`, wrapping modulo 7 (so `-1` is Si).
    pub fn from_index(i: i32) -> Degree {
        Self::ALL[i.rem_euclid(7) as usize]
    }

    /// Semitones of the natural degree above Do (Do 0, Re 2, Mi 4, Fa 5,
    /// Sol 7, La 9, Si 11).
    pub fn natural_semitones(self) -> u8 {
        [0, 2, 4, 5, 7, 9, 11][self as usize]
    }

    /// Latin-script name.
    pub fn latin(self) -> &'static str {
        ["Do", "Re", "Mi", "Fa", "Sol", "La", "Si"][self as usize]
    }

    /// Arabic-script name.
    pub fn arabic(self) -> &'static str {
        ["دو", "ري", "مي", "فا", "صول", "لا", "سي"][self as usize]
    }
}

/// An accidental, in signed quarter-tone steps from the natural degree
/// (−4 = double flat … +4 = double sharp).
///
/// In 24-EDO one step is exactly 50 cents. Under other intonation profiles
/// the *name* stays the same while the target pitch may differ.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(try_from = "i8", into = "i8")]
pub struct Accidental(i8);

impl Accidental {
    /// −4 quarter-tones.
    pub const DOUBLE_FLAT: Accidental = Accidental(-4);
    /// −3 quarter-tones.
    pub const FLAT_AND_A_HALF: Accidental = Accidental(-3);
    /// −2 quarter-tones.
    pub const FLAT: Accidental = Accidental(-2);
    /// −1 quarter-tone.
    pub const HALF_FLAT: Accidental = Accidental(-1);
    /// No alteration.
    pub const NATURAL: Accidental = Accidental(0);
    /// +1 quarter-tone.
    pub const HALF_SHARP: Accidental = Accidental(1);
    /// +2 quarter-tones.
    pub const SHARP: Accidental = Accidental(2);
    /// +3 quarter-tones.
    pub const SHARP_AND_A_HALF: Accidental = Accidental(3);
    /// +4 quarter-tones.
    pub const DOUBLE_SHARP: Accidental = Accidental(4);

    /// Accidental of `quarter_tones` steps; `None` outside `-4..=4`.
    pub fn new(quarter_tones: i8) -> Option<Self> {
        (-4..=4)
            .contains(&quarter_tones)
            .then_some(Self(quarter_tones))
    }

    /// Signed quarter-tone steps.
    pub fn quarter_tones(self) -> i8 {
        self.0
    }

    /// Nominal size in equal-tempered semitones (`quarter_tones / 2`).
    pub fn semitones(self) -> f64 {
        self.0 as f64 * 0.5
    }

    /// `true` for the natural (unaltered) accidental.
    pub fn is_natural(self) -> bool {
        self.0 == 0
    }

    /// Unicode musical symbol(s). Empty for natural.
    ///
    /// Quarter-tone symbols are U+1D133 (𝄳, quarter-tone flat) and U+1D132
    /// (𝄲, quarter-tone sharp); many system fonts lack them, so UIs should
    /// bundle a music font or use [`quarter_tones`](Self::quarter_tones) to
    /// draw their own glyphs.
    pub fn symbol(self) -> &'static str {
        match self.0 {
            -4 => "𝄫",
            -3 => "♭𝄳",
            -2 => "♭",
            -1 => "𝄳",
            1 => "𝄲",
            2 => "♯",
            3 => "♯𝄲",
            4 => "𝄪",
            _ => "",
        }
    }

    /// English name. Empty for natural. *(−3/+3 names are provisional.)*
    pub fn english_name(self) -> &'static str {
        match self.0 {
            -4 => "double flat",
            -3 => "flat-and-a-half",
            -2 => "flat",
            -1 => "half-flat",
            1 => "half-sharp",
            2 => "sharp",
            3 => "sharp-and-a-half",
            4 => "double sharp",
            _ => "",
        }
    }

    /// Arabic name. Empty for natural. *(Provisional terminology.)*
    pub fn arabic_name(self) -> &'static str {
        match self.0 {
            -4 => "دوبل بيمول",
            -3 => "بيمول ونصف",
            -2 => "بيمول",
            -1 => "نصف بيمول",
            1 => "نصف دييز",
            2 => "دييز",
            3 => "دييز ونصف",
            4 => "دوبل دييز",
            _ => "",
        }
    }
}

impl TryFrom<i8> for Accidental {
    type Error = NotationError;
    fn try_from(q: i8) -> Result<Self, Self::Error> {
        Accidental::new(q).ok_or(NotationError::AccidentalOutOfRange(q))
    }
}

impl From<Accidental> for i8 {
    fn from(a: Accidental) -> i8 {
        a.0
    }
}

/// A note name: degree, accidental and octave, e.g. *Mi half-flat 4*.
///
/// Serialised as its canonical text (`"Do2"`, `"Mi half-flat 4"`), which
/// [`FromStr`] parses back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SpelledNote {
    /// Natural degree.
    pub degree: Degree,
    /// Alteration in quarter-tones.
    pub accidental: Accidental,
    /// Octave of the spelled degree (Do4 = middle Do).
    pub octave: i8,
}

impl SpelledNote {
    /// Construct a note.
    pub const fn new(degree: Degree, accidental: Accidental, octave: i8) -> Self {
        Self {
            degree,
            accidental,
            octave,
        }
    }

    /// Natural (unaltered) note.
    pub const fn natural(degree: Degree, octave: i8) -> Self {
        Self::new(degree, Accidental::NATURAL, octave)
    }

    /// Nominal equal-tempered pitch (24-EDO grid): Do4 = 60, quarter-tones
    /// at x.5.
    pub fn nominal_pitch(&self) -> Pitch {
        Pitch(
            12.0 * (self.octave as f64 + 1.0)
                + self.degree.natural_semitones() as f64
                + self.accidental.semitones(),
        )
    }

    /// Render a label for display.
    pub fn label(&self, options: &LabelOptions) -> NoteLabel {
        let name = match options.script {
            Script::Latin => self.degree.latin(),
            Script::Arabic => self.degree.arabic(),
        };
        let accidental = match (options.accidentals, options.script) {
            (AccidentalStyle::Symbol, _) => self.accidental.symbol(),
            (AccidentalStyle::Words, Script::Latin) => self.accidental.english_name(),
            (AccidentalStyle::Words, Script::Arabic) => self.accidental.arabic_name(),
        };
        let octave = options
            .show_octave
            .then(|| format_number(self.octave as i64, options.digits));

        let mut text = String::from(name);
        if !accidental.is_empty() {
            if options.accidentals == AccidentalStyle::Words {
                text.push(' ');
            }
            text.push_str(accidental);
        }
        if let Some(o) = &octave {
            if options.accidentals == AccidentalStyle::Words && !accidental.is_empty() {
                text.push(' ');
            }
            text.push_str(o);
        }
        NoteLabel {
            text,
            name: name.to_string(),
            accidental: accidental.to_string(),
            octave: octave.unwrap_or_default(),
            degree: self.degree,
            accidental_quarter_tones: self.accidental.quarter_tones(),
            octave_number: self.octave,
        }
    }
}

impl fmt::Display for SpelledNote {
    /// Canonical Latin text: `Do2`, `Mi half-flat 4`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.accidental.is_natural() {
            write!(f, "{}{}", self.degree.latin(), self.octave)
        } else {
            write!(
                f,
                "{} {} {}",
                self.degree.latin(),
                self.accidental.english_name(),
                self.octave
            )
        }
    }
}

impl From<SpelledNote> for String {
    fn from(n: SpelledNote) -> String {
        n.to_string()
    }
}

impl TryFrom<String> for SpelledNote {
    type Error = NotationError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl FromStr for SpelledNote {
    type Err = NotationError;

    /// Parse `<degree>[accidental]<octave>`.
    ///
    /// The degree is a Latin or Arabic solfège name (case-insensitive for
    /// Latin). The accidental may be a Unicode symbol (♭ ♯ 𝄳 𝄲 𝄫 𝄪), an
    /// English name (`half-flat`, `flat`, …), or ASCII `b`/`#`/`bb`/`##`.
    /// The octave is a (possibly negative) integer.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || NotationError::InvalidNote(s.to_string());
        let t = s.trim();

        // Longest-first so "Sol" isn't read as "So…" and "Si" vs "Sol" etc.
        let (degree, rest) = Degree::ALL
            .iter()
            .flat_map(|&d| [(d, d.latin()), (d, d.arabic())])
            .filter_map(|(d, name)| {
                let head = t.get(..name.len())?;
                head.eq_ignore_ascii_case(name)
                    .then(|| (d, &t[name.len()..]))
            })
            .max_by_key(|(_, rest)| usize::MAX - rest.len())
            .ok_or_else(invalid)?;

        // Trailing octave number.
        let digits_start = rest
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_digit())
            .last()
            .map(|(i, _)| i)
            .ok_or_else(invalid)?;
        let mut octave_start = digits_start;
        if rest[..digits_start].ends_with('-') {
            octave_start -= 1;
        }
        let octave: i8 = rest[octave_start..].parse().map_err(|_| invalid())?;
        let acc_text = rest[..octave_start].trim();
        let accidental = parse_accidental(acc_text).ok_or_else(invalid)?;
        Ok(SpelledNote::new(degree, accidental, octave))
    }
}

fn parse_accidental(s: &str) -> Option<Accidental> {
    if s.is_empty() {
        return Some(Accidental::NATURAL);
    }
    let lower = s.to_ascii_lowercase();
    (-4..=4)
        .map(Accidental)
        .find(|a| lower == a.english_name() || s == a.symbol() || s == a.arabic_name())
        .or(match s {
            "bb" => Some(Accidental::DOUBLE_FLAT),
            "b" => Some(Accidental::FLAT),
            "#" => Some(Accidental::SHARP),
            "##" | "x" => Some(Accidental::DOUBLE_SHARP),
            "natural" | "♮" => Some(Accidental::NATURAL),
            _ => None,
        })
}

/// Script for note names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Script {
    /// Do, Re, Mi, Fa, Sol, La, Si.
    #[default]
    Latin,
    /// دو، ري، مي، فا، صول، لا، سي.
    Arabic,
}

/// How accidentals are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccidentalStyle {
    /// Musical symbols: ♭ ♯ 𝄳 𝄲 …
    #[default]
    Symbol,
    /// Words in the label's script: "half-flat" / "نصف بيمول".
    Words,
}

/// Digit set for octave numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Digits {
    /// 0123456789
    #[default]
    Western,
    /// ٠١٢٣٤٥٦٧٨٩
    ArabicIndic,
}

/// Options for [`SpelledNote::label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelOptions {
    /// Script for the degree name (and accidental words).
    pub script: Script,
    /// Accidental rendering.
    pub accidentals: AccidentalStyle,
    /// Include the octave number.
    pub show_octave: bool,
    /// Digits for the octave number.
    pub digits: Digits,
}

impl Default for LabelOptions {
    fn default() -> Self {
        Self {
            script: Script::Latin,
            accidentals: AccidentalStyle::Symbol,
            show_octave: true,
            digits: Digits::Western,
        }
    }
}

/// A rendered note label, with its parts kept separate so UIs can style
/// them individually (e.g. a large name with a smaller accidental).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLabel {
    /// Full label text, e.g. `Mi𝄳4` or `مي نصف بيمول ٤`.
    pub text: String,
    /// Degree name in the chosen script.
    pub name: String,
    /// Accidental text (symbol or words); empty for natural.
    pub accidental: String,
    /// Octave text; empty if octaves are hidden.
    pub octave: String,
    /// Degree.
    pub degree: Degree,
    /// Accidental in quarter-tone steps (for custom glyph rendering).
    pub accidental_quarter_tones: i8,
    /// Octave number.
    pub octave_number: i8,
}

fn format_number(n: i64, digits: Digits) -> String {
    let s = n.to_string();
    match digits {
        Digits::Western => s,
        Digits::ArabicIndic => s
            .chars()
            .map(|c| match c.to_digit(10) {
                Some(d) => char::from_u32(0x0660 + d).unwrap_or(c),
                None => c,
            })
            .collect(),
    }
}

/// Errors from parsing or constructing notation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotationError {
    /// Accidental outside −4..=4 quarter-tones.
    AccidentalOutOfRange(i8),
    /// Text that is not a valid note name.
    InvalidNote(String),
}

impl fmt::Display for NotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccidentalOutOfRange(q) => {
                write!(f, "accidental of {q} quarter-tones is outside -4..=4")
            }
            Self::InvalidNote(s) => write!(f, "not a valid solfège note: {s:?}"),
        }
    }
}

impl std::error::Error for NotationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> SpelledNote {
        s.parse().unwrap()
    }

    #[test]
    fn nominal_pitches() {
        assert_eq!(n("Do4").nominal_pitch(), Pitch(60.0));
        assert_eq!(n("La4").nominal_pitch(), Pitch(69.0));
        assert_eq!(n("Do2").nominal_pitch(), Pitch(36.0));
        assert_eq!(n("Mi half-flat 4").nominal_pitch(), Pitch(63.5));
        assert_eq!(n("Si flat 3").nominal_pitch(), Pitch(58.0));
        assert_eq!(n("Fa sharp 4").nominal_pitch(), Pitch(66.0));
    }

    #[test]
    fn octave_follows_spelled_degree() {
        // Do half-flat 4 sounds below Do4 yet is written in octave 4, and is
        // the same pitch as Si half-sharp 3.
        let do_hf = n("Do half-flat 4");
        assert_eq!(do_hf.octave, 4);
        assert!(do_hf.nominal_pitch() < n("Do4").nominal_pitch());
        assert_eq!(do_hf.nominal_pitch(), n("Si half-sharp 3").nominal_pitch());
    }

    #[test]
    fn labels_latin_and_arabic() {
        let note = n("Mi half-flat 4");
        let latin = note.label(&LabelOptions::default());
        assert_eq!(latin.text, "Mi𝄳4");
        assert_eq!(latin.name, "Mi");
        assert_eq!(latin.accidental_quarter_tones, -1);

        let words = note.label(&LabelOptions {
            accidentals: AccidentalStyle::Words,
            ..LabelOptions::default()
        });
        assert_eq!(words.text, "Mi half-flat 4");

        let arabic = note.label(&LabelOptions {
            script: Script::Arabic,
            accidentals: AccidentalStyle::Words,
            digits: Digits::ArabicIndic,
            show_octave: true,
        });
        assert_eq!(arabic.text, "مي نصف بيمول ٤");

        let re4 = n("Re4").label(&LabelOptions {
            script: Script::Arabic,
            ..LabelOptions::default()
        });
        assert_eq!(re4.text, "ري4");

        let no_oct = n("Sol3").label(&LabelOptions {
            show_octave: false,
            ..LabelOptions::default()
        });
        assert_eq!(no_oct.text, "Sol");
    }

    #[test]
    fn no_western_letters_in_any_label() {
        // The musician-facing labels must never fall back to C D E F G A B.
        for &d in &Degree::ALL {
            for q in -4..=4 {
                let note = SpelledNote::new(d, Accidental::new(q).unwrap(), 3);
                for script in [Script::Latin, Script::Arabic] {
                    for accidentals in [AccidentalStyle::Symbol, AccidentalStyle::Words] {
                        let l = note.label(&LabelOptions {
                            script,
                            accidentals,
                            ..LabelOptions::default()
                        });
                        let solfege: Vec<&str> = Degree::ALL
                            .iter()
                            .flat_map(|d| [d.latin(), d.arabic()])
                            .collect();
                        assert!(solfege.contains(&l.name.as_str()), "{}", l.text);
                        assert!(l.text.starts_with(&l.name));
                    }
                }
            }
        }
    }

    #[test]
    fn parse_variants_and_round_trip() {
        assert_eq!(n("sol3"), SpelledNote::natural(Degree::Sol, 3));
        assert_eq!(n("Si♭3"), n("Si flat 3"));
        assert_eq!(n("Sib3"), n("Si flat 3"));
        assert_eq!(n("Fa#2"), n("Fa sharp 2"));
        assert_eq!(n("Mi𝄳4"), n("Mi half-flat 4"));
        assert_eq!(n("Do-1"), SpelledNote::natural(Degree::Do, -1));
        assert_eq!(n("صول3"), n("Sol3"));
        assert!("Ca4".parse::<SpelledNote>().is_err());
        assert!("Do".parse::<SpelledNote>().is_err());
        assert!("Do quarter 4".parse::<SpelledNote>().is_err());

        for &d in &Degree::ALL {
            for q in -4..=4 {
                for o in [-1, 0, 3, 9] {
                    let note = SpelledNote::new(d, Accidental::new(q).unwrap(), o);
                    assert_eq!(note.to_string().parse::<SpelledNote>().unwrap(), note);
                    let json = serde_json::to_string(&note).unwrap();
                    assert_eq!(serde_json::from_str::<SpelledNote>(&json).unwrap(), note);
                }
            }
        }
        assert_eq!(
            serde_json::to_string(&n("Mi half-flat 4")).unwrap(),
            "\"Mi half-flat 4\""
        );
    }

    #[test]
    fn accidental_validation() {
        assert_eq!(Accidental::new(5), None);
        assert!(serde_json::from_str::<Accidental>("-5").is_err());
        assert_eq!(
            serde_json::from_str::<Accidental>("-1").unwrap(),
            Accidental::HALF_FLAT
        );
    }
}
