//! The `bpmn.pl` DSL vocabulary (see the header of `prolog/bpmn.pl`), shared
//! by syntax highlighting and completion.

/// What kind of DSL term an [`Entry`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Category {
    /// A model, pool, lane, event, activity or gateway.
    Element,
    /// A trigger, task kind or flow label.
    Value,
}

/// One completion template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) name: &'static str,
    pub(crate) template: &'static str,
    pub(crate) detail: &'static str,
    pub(crate) category: Category,
    /// Whether the first argument declares an element id that flows can
    /// refer to.
    pub(crate) declares_id: bool,
}

const fn element(name: &'static str, template: &'static str, detail: &'static str) -> Entry {
    Entry {
        name,
        template,
        detail,
        category: Category::Element,
        declares_id: true,
    }
}

const fn container(name: &'static str, template: &'static str, detail: &'static str) -> Entry {
    Entry {
        declares_id: false,
        ..element(name, template, detail)
    }
}

const fn value(name: &'static str, template: &'static str, detail: &'static str) -> Entry {
    Entry {
        name,
        template,
        detail,
        category: Category::Value,
        declares_id: false,
    }
}

pub(crate) const ENTRIES: &[Entry] = &[
    container("bpmn", "bpmn(Id, [])", "model"),
    element("pool", "pool(Id, \"Name\", [])", "pool with process"),
    element("pool", "pool(Id, \"Name\")", "black-box pool"),
    container("process", "process(Id, \"Name\", [])", "process"),
    container("lane", "lane(Id, \"Name\", [])", "lane"),
    element("start", "start(Id, \"Name\")", "start event"),
    element("start", "start(Id, \"Name\", message)", "start event"),
    element("end", "end(Id, \"Name\")", "end event"),
    element("catch", "catch(Id, \"Name\", message)", "catch event"),
    element("throw", "throw(Id, \"Name\", message)", "throw event"),
    element(
        "boundary",
        "boundary(Id, \"Name\", timer(\"PT1H\"), Activity)",
        "boundary event",
    ),
    element("task", "task(Id, \"Name\")", "task"),
    element("task", "task(Id, \"Name\", user)", "task"),
    element("subprocess", "subprocess(Id, \"Name\", [])", "sub-process"),
    element("call", "call(Id, \"Name\", Called)", "call activity"),
    element("xor", "xor(Id, \"Name\")", "exclusive gateway"),
    element("and", "and(Id)", "parallel gateway"),
    element("or", "or(Id, \"Name\")", "inclusive gateway"),
    element("event_based", "event_based(Id)", "event-based gateway"),
    element("complex", "complex(Id)", "complex gateway"),
    value("none", "none", "trigger"),
    value("message", "message", "trigger"),
    value("signal", "signal", "trigger"),
    value("error", "error", "trigger"),
    value("escalation", "escalation", "trigger"),
    value("compensation", "compensation", "trigger"),
    value("terminate", "terminate", "trigger"),
    value("timer", "timer(\"PT1H\")", "timer trigger"),
    value("conditional", "conditional(\"${condition}\")", "trigger"),
    value("link", "link(\"Name\")", "trigger"),
    value(
        "non_interrupting",
        "non_interrupting(timer(\"PT1H\"))",
        "boundary trigger",
    ),
    value("user", "user", "task kind"),
    value("service", "service", "task kind"),
    value("script", "script", "task kind"),
    value("send", "send", "task kind"),
    value("receive", "receive", "task kind"),
    value("manual", "manual", "task kind"),
    value("business_rule", "business_rule", "task kind"),
    value("default", "default", "flow label"),
    value("when", "when(\"label\", \"${condition}\")", "flow label"),
];

/// The entries named `name`.
pub(crate) fn lookup(name: &str) -> impl Iterator<Item = &'static Entry> {
    ENTRIES.iter().filter(move |entry| entry.name == name)
}

impl Entry {
    /// The byte range of the template's first argument, which is selected
    /// after insertion so it can be typed over.
    pub(crate) fn first_argument(&self) -> Option<std::ops::Range<usize>> {
        let open = self.template.find('(')? + 1;
        let close = open + self.template[open..].find([',', ')'])?;
        Some(open..close)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_lexically_valid_terms() {
        for entry in ENTRIES {
            let clause = format!("x({}).", entry.template);
            assert!(bpmn_syntax::check(&clause).is_ok(), "{}", entry.template);
            assert!(entry.template.starts_with(entry.name), "{}", entry.template);
        }
    }

    #[test]
    fn selects_the_first_argument() {
        let task = lookup("task").next().unwrap();
        assert_eq!(&task.template[task.first_argument().unwrap()], "Id");
        let timer = lookup("timer").next().unwrap();
        assert_eq!(&timer.template[timer.first_argument().unwrap()], "\"PT1H\"");
        assert_eq!(lookup("user").next().unwrap().first_argument(), None);
    }
}
