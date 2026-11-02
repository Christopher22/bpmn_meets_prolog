//! End-to-end tests of `Engine::render` on a native Scryer machine.

use bpmn_engine::{Engine, Outcome, Rendering};

const PIZZA: &str = include_str!("../../../prolog/pizza.pl");

fn render(engine: &mut Engine, source: &str) -> Rendering {
    let program = bpmn_syntax::check(source).expect("test programs are lexically valid");
    engine.render(&program).expect("the driver never fails")
}

fn pizza_model() -> String {
    PIZZA.replacen("pizza(bpmn(", "model(bpmn(", 1)
}

#[test]
fn renders_the_pizza_collaboration() {
    let mut engine = Engine::new();
    let rendering = render(&mut engine, &pizza_model());

    // initialization/1 is absent, so the only diagnostics would be real ones.
    assert_eq!(rendering.diagnostics, []);
    let Outcome::Rendered { xml, problems } = rendering.outcome else {
        panic!("unexpected outcome: {:?}", rendering.outcome);
    };
    assert_eq!(problems, []);
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<bpmn:participant id=\"customer\" name=\"Pizza Customer\""));
    assert!(xml.contains("<bpmn:eventBasedGateway id=\"wait\""));
    assert!(xml.contains("<bpmndi:BPMNShape id=\"bake_di\" bpmnElement=\"bake\""));
}

#[test]
fn reports_a_missing_model() {
    let mut engine = Engine::new();
    let rendering = render(&mut engine, PIZZA);
    assert_eq!(rendering.outcome, Outcome::NoModel);
}

#[test]
fn reports_check_problems() {
    let mut engine = Engine::new();
    let rendering = render(
        &mut engine,
        ":- use_module(bpmn).\nmodel(bpmn(m, [process(p, \"P\", [start(s), task(t, \"T\"), end(e), s => t])])).",
    );
    let Outcome::Rendered { problems, .. } = rendering.outcome else {
        panic!("unexpected outcome: {:?}", rendering.outcome);
    };
    let codes: Vec<_> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(
        codes,
        [
            "e has no incoming sequence flow",
            "t has no outgoing sequence flow"
        ]
    );
    assert_eq!(problems[0].element(), Some("e"));
}

#[test]
fn replaces_the_previous_program() {
    let mut engine = Engine::new();
    render(&mut engine, &pizza_model());
    let rendering = render(&mut engine, ":- use_module(bpmn).\nother(1).");
    assert_eq!(rendering.outcome, Outcome::NoModel);

    // A helper from the first program must not leak into the second.
    let rendering = render(&mut engine, "model(M) :- report([]), M = x.");
    assert_eq!(
        rendering.outcome,
        Outcome::Failed {
            message: "existence_error(procedure,report/1)".to_owned()
        }
    );
}

#[test]
fn keeps_loading_after_bad_clauses() {
    let mut engine = Engine::new();
    let rendering = render(
        &mut engine,
        "\
:- use_module(bpmn).
:- use_module(somewhere_else).
:- initialization(main).
broken(a b).
:- fail.
model(M) :- items(Is), M = bpmn(m, [process(p, \"P\", Is)]).
items([start(s), end(e), s => e]).
",
    );
    let lines: Vec<_> = rendering.diagnostics.iter().map(|d| d.line).collect();
    assert_eq!(lines, [2, 3, 4, 5], "{:#?}", rendering.diagnostics);
    assert!(
        rendering.diagnostics[3]
            .message
            .starts_with("directive failed")
    );
    assert!(
        matches!(rendering.outcome, Outcome::Rendered { ref problems, .. } if problems.is_empty())
    );
}

#[test]
fn reports_malformed_models() {
    let mut engine = Engine::new();
    let rendering = render(
        &mut engine,
        ":- use_module(bpmn).\nmodel(bpmn(m, [nonsense(x)])).",
    );
    let Outcome::Failed { message } = rendering.outcome else {
        panic!("unexpected outcome: {:?}", rendering.outcome);
    };
    assert!(
        message.contains("domain_error(bpmn_top_item,nonsense(x)"),
        "{message}"
    );

    let rendering = render(&mut engine, "model(not_a_model).");
    assert_eq!(
        rendering.outcome,
        Outcome::Failed {
            message: "model/1 must yield bpmn(Id, Items), not not_a_model".to_owned()
        }
    );

    let rendering = render(&mut engine, "model(_) :- fail.");
    assert_eq!(
        rendering.outcome,
        Outcome::Failed {
            message: "model/1 has no solution".to_owned()
        }
    );
}

#[test]
fn supports_libraries_dcgs_and_output() {
    let mut engine = Engine::new();
    let rendering = render(
        &mut engine,
        "\
:- use_module(bpmn).
:- use_module(library(lists)).
:- use_module(library(format)).
:- use_module(library(dcgs)).
steps([a, b, c]).
ids([]) --> [].
ids([X|Xs]) --> [X], ids(Xs).
model(bpmn(m, [process(p, \"P\", Items)])) :-
    steps(Ss), phrase(ids(Ss), Ids),
    findall(task(T, \"Step\"), member(T, Ids), Tasks),
    append([[start(s)], Tasks, [end(e)]], Body),
    append(Body, [s => a => b => c => e], Items).
:- steps(Ss), length(Ss, N), format(\"~d steps~n\", [N]).
",
    );
    assert_eq!(rendering.diagnostics, []);
    assert_eq!(rendering.output, "3 steps\n");
    let Outcome::Rendered { xml, problems } = rendering.outcome else {
        panic!("unexpected outcome: {:?}", rendering.outcome);
    };
    assert_eq!(problems, []);
    assert!(xml.contains("<bpmn:task id=\"b\" name=\"Step\""));
}

#[test]
fn reports_syntax_errors_found_by_prolog() {
    let mut engine = Engine::new();
    // Lexically fine, but not a term.
    let rendering = render(&mut engine, "a :- .\nmodel(x).");
    assert_eq!(rendering.diagnostics.len(), 1);
    assert_eq!(rendering.diagnostics[0].line, 1);
    assert!(rendering.diagnostics[0].message.starts_with("syntax error"));
}

#[test]
fn renders_large_models_completely_but_truncates_output() {
    let mut engine = Engine::new();
    let rendering = render(
        &mut engine,
        "\
:- use_module(bpmn).
:- use_module(library(lists)).
:- use_module(library(between)).
:- use_module(library(format)).
model(bpmn(big, [process(p, \"Big\", [start(s), end(e) | Items])])) :-
    numlist(1, 400, Ns),
    maplist(task_id, Ns, Ids),
    findall(task(T, \"A task with a fairly long name\"), member(T, Ids), Tasks),
    append([s | Ids], [e], Path),
    chain(Path, Flow),
    append(Tasks, [Flow], Items).
task_id(N, T) :- number_chars(N, Cs), atom_chars(T, [t | Cs]).
chain([X], X).
chain([X, Y | Zs], X => Rest) :- chain([Y | Zs], Rest).
:- between(1, 20000, _), format(\"spam \", []), false ; true.
",
    );
    assert_eq!(rendering.diagnostics, [], "{:#?}", rendering.diagnostics);
    assert!(rendering.output.len() < 70_000);
    assert!(rendering.output.ends_with("… output truncated"));
    let Outcome::Rendered { xml, problems } = rendering.outcome else {
        panic!("unexpected outcome: {:?}", rendering.outcome);
    };
    assert_eq!(problems, []);
    assert!(xml.len() > 100_000, "{}", xml.len());
    assert!(xml.trim_end().ends_with("</bpmn:definitions>"));
}

#[test]
fn routes_long_flows_around_shapes() {
    // g => b skips the column of a, so the flow must not run through a.
    let mut engine = Engine::new();
    let rendering = render(
        &mut engine,
        ":- use_module(bpmn).\nmodel(bpmn(m, [process(p, \"P\", [start(s), xor(g), \
         task(a, \"A\"), task(b, \"B\"), end(e), s => g => [a, b], a => b => e])])).",
    );
    let Outcome::Rendered { xml, problems } = rendering.outcome else {
        panic!("unexpected outcome: {:?}", rendering.outcome);
    };
    assert_eq!(problems, []);

    let shape = element(&xml, "a_di");
    let [x, y, w, h] = ["x", "y", "width", "height"].map(|name| number(shape, name));
    let points: Vec<_> = element(&xml, "flow_g_b_di")
        .split("<di:waypoint")
        .skip(1)
        .map(|p| (number(p, "x"), number(p, "y")))
        .collect();
    assert!(points.len() > 2, "{points:?}");
    for pair in points.windows(2) {
        let [(x1, y1), (x2, y2)] = [pair[0], pair[1]];
        let crosses = x1.min(x2) < x + w && x1.max(x2) > x && y1.min(y2) < y + h && y1.max(y2) > y;
        assert!(!crosses, "{points:?} crosses {:?}", (x, y, w, h));
    }
}

/// The text of the diagram element with the given id, up to its end tag.
fn element<'a>(xml: &'a str, id: &str) -> &'a str {
    let start = xml.find(&format!("id=\"{id}\"")).expect("element exists");
    let rest = &xml[start..];
    &rest[..rest.find("</bpmndi:").expect("element is closed")]
}

/// The first numeric attribute `name` in `text`.
fn number(text: &str, name: &str) -> i64 {
    let start = text.find(&format!(" {name}=\"")).expect("attribute exists") + name.len() + 3;
    let len = text[start..].find('"').expect("attribute is closed");
    text[start..start + len]
        .parse()
        .expect("attribute is a number")
}
