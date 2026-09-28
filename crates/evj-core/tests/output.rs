use evj_core::model::Project;
use evj_core::output::{OutputConfig, OutputSource, Slice};

#[test]
fn default_project_has_one_full_output() {
    let p = Project::new_default();
    assert_eq!(p.outputs.len(), 1);
    let o = &p.outputs[0];
    assert_eq!(o.source, OutputSource::Composition);
    assert_eq!(o.slices, vec![Slice::full()]);
}

#[test]
fn slice_rects_are_clamped_and_serialized() {
    let s = Slice { name: "LED left".into(), input: [-0.2, 0.0, 0.7, 1.5], output: [0.0, 0.0, 1.0, 1.0] }.clamped();
    assert_eq!(s.input, [0.0, 0.0, 0.7, 1.0]);
    let o = OutputConfig { name: "Side".into(), source: OutputSource::Layer(2), slices: vec![s.clone()], ..OutputConfig::default() };
    let back: OutputConfig = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
    assert_eq!(back, o);
}

#[test]
fn split_in_columns() {
    let v = Slice::columns(3);
    assert_eq!(v.len(), 3);
    assert_eq!(v[1].input, [1.0 / 3.0, 0.0, 1.0 / 3.0, 1.0]);
    assert_eq!(v[1].output, v[1].input, "each column lands where it came from");
}
