//! Export every documented illustration on all 24 tonic classes for offline reference.
use naghma_core::maqam::MaqamDataset;
use naghma_core::notation::{AccidentalStyle, LabelOptions, Script, SpelledNote};
use naghma_core::tuning::SpellingTable;
use serde_json::json;

fn labels(note: SpelledNote) -> serde_json::Value {
    let latin = LabelOptions {
        show_octave: false,
        accidentals: AccidentalStyle::Words,
        ..Default::default()
    };
    let arabic = LabelOptions {
        script: Script::Arabic,
        ..latin
    };
    json!({"en":note.label(&latin).text,"ar":note.label(&arabic).text})
}

fn main() {
    let dataset = MaqamDataset::teaching_eight_24edo();
    let table = SpellingTable::default_24();
    let tonics: Vec<_> = (0..24)
        .map(|i| {
            let s = table.get(i);
            labels(SpelledNote::new(s.degree, s.accidental, 3))
        })
        .collect();
    let maqamat:Vec<_> = dataset.maqamat.iter().map(|m| {
        let variants:Vec<_> = std::iter::once(None).chain(m.variants.iter().map(|v|Some(v.id.as_str()))).map(|id| {
            let title=id.and_then(|id|m.variants.iter().find(|v|v.id==id)).map_or(json!({"latin":"Primary source illustration","arabic":"الصورة الأساسية في المصدر"}),|v|json!(v.name));
            let transpositions:Vec<_> = (0..24).map(|i| {
                let s=table.get(i);let root=SpelledNote::new(s.degree,s.accidental,3);
                let notes:Vec<_> = m.realize_variant(root,id).unwrap().iter().zip(m.scale(id).unwrap()).map(|(n,d)|json!({"name":labels(n.note),"cents":d.cents,"step":d.step})).collect();
                json!(notes)
            }).collect();
            let ajnas:Vec<_> = m.placements(id).unwrap().iter().map(|p| {
                let jins=dataset.jins(&p.jins).unwrap();
                json!({"name":jins.name,"degree":p.on_step+1,"tonicDegree":p.on_step+jins.tonic_step+1})
            }).collect();
            json!({"id":id,"name":title,"notes":transpositions,"ajnas":ajnas})
        }).collect();
        json!({"id":m.id,"name":m.name,"referenceTonic":m.tonic.quarter_tone_class(),"repeating":m.octave_repeating,"variants":variants,"sources":m.sources})
    }).collect();
    println!("{}", json!({"tonics":tonics,"maqamat":maqamat}));
}
