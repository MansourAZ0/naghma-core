// Types mirroring naghma-core's serde output (camelCase).

export type Degree = "do" | "re" | "mi" | "fa" | "sol" | "la" | "si";

/** A note label rendered by naghma-core (`SpelledNote::label`). */
export interface NoteLabel {
  text: string;
  name: string;
  accidental: string;
  octave: string;
  degree: Degree;
  accidentalQuarterTones: number;
  octaveNumber: number;
}

export type Script = "latin" | "arabic";
export type AccidentalStyle = "symbol" | "words";
export type NoteView = "player" | "concert";
export type IntervalUnit = "semitone" | "wholeTone" | "quarterTone" | "cent";

export interface Transposition {
  amount: number;
  unit: IntervalUnit;
}
