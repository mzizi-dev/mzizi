//! `mz outline` — a component's interface, at a fraction of its source cost.
//!
//! This is the answer to RB-2 (file-granular reads): when editing component A that uses B,
//! an agent needs B's *interface*, not B's body. Reading the file gets both, plus imports
//! and comments and unrelated functions.
//!
//! The outline is emitted as **valid Mzizi** — a component with everything except bodies.
//! That is deliberate: a reader who can read the language can already read this, so there
//! is no second format to learn and no second parser to keep in step (RFC-0003 §4).

use crate::ast::Component;

/// Render a component's interface.
pub fn outline(component: &Component) -> String {
    let mut out = String::with_capacity(256);

    if let Some(first) = component.docs.first() {
        out.push_str("## ");
        out.push_str(first);
        out.push('\n');
    }

    out.push_str("component ");
    out.push_str(&component.name);
    out.push('\n');

    for cap in &component.uses {
        out.push_str("  use ");
        out.push_str(cap);
        out.push('\n');
    }

    for p in &component.props {
        out.push_str("  prop ");
        out.push_str(&p.name);
        out.push_str(": ");
        out.push_str(&p.ty);
        if let Some(value) = &p.default {
            // The default value is interface: a caller needs to know what happens when the
            // prop is omitted. It is also short, so it costs nothing to keep.
            out.push_str(" = ");
            out.push_str(value);
        }
        out.push('\n');
    }

    // Variant *names* are interface — a caller must know what it may pass. The column data
    // behind them is implementation, and is where the bulk of a variant table's bytes are.
    for e in &component.enums {
        out.push_str("  enum ");
        out.push_str(&e.name);
        out.push('\n');
        for v in &e.variants {
            out.push_str("    ");
            out.push_str(&v.name);
            out.push('\n');
        }
        out.push_str("  end\n");
    }

    for f in &component.fns {
        out.push_str("  fn ");
        out.push_str(f);
        out.push_str("\n  end\n");
    }

    if component.has_contract {
        out.push_str("  contract\n  end\n");
    }

    out.push_str("end component ");
    out.push_str(&component.name);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_with_ast;

    fn parse(src: &str) -> Component {
        let (c, report) = check_with_ast(src, "t.mz");
        assert_eq!(report.error_count(), 0);
        c.unwrap()
    }

    #[test]
    fn the_outline_keeps_the_interface_and_drops_the_body() {
        let src = "\
## A button.
component button
  prop label: text
  prop size: button_size = default
  enum button_size
    default  class \"h-14 gap-2 px-5 text-sm font-medium\"  height 56
    sm       class \"h-12 gap-1.5 px-4 text-sm\"            height 48
  end
  view
    control
      class = \"a very long class string that is implementation detail\"
      text = label
    end
  end
  contract
    every button_size height at_least 48
  end
end component button
";
        let text = outline(&parse(src));
        // Interface survives.
        assert!(text.contains("prop label: text"));
        assert!(text.contains("prop size: button_size = default"));
        assert!(text.contains("enum button_size\n    default\n    sm\n  end"));
        assert!(text.contains("contract"));
        // Implementation does not.
        assert!(
            !text.contains("h-14"),
            "class strings are body, not interface"
        );
        assert!(!text.contains("height 56"), "column data is body");
        assert!(!text.contains("control"), "the view tree is body");
    }

    #[test]
    fn the_outline_is_itself_parseable_mzizi() {
        // The property that means an agent needs no second format (RFC-0003 §4). If this
        // breaks, the outline has stopped being self-describing.
        let src = "\
component a
  use motion
  prop x: bool = true
  enum e
    one k \"1\"
    two k \"2\"
  end
  fn go
  end
  contract
    x is true
  end
end component a
";
        let text = outline(&parse(src));
        let (reparsed, report) = check_with_ast(&text, "outline.mz");
        assert_eq!(
            report.error_count(),
            0,
            "outline must parse: {}\n{text}",
            report.error_count()
        );
        let r = reparsed.expect("outline parses to a component");
        assert_eq!(r.name, "a");
        assert_eq!(r.uses, vec!["motion"]);
        assert_eq!(r.props.len(), 1);
        assert!(r.has_contract);
    }

    #[test]
    fn a_capability_is_interface_because_it_is_blast_radius() {
        let text = outline(&parse(
            "component a\n  use net\n  contract\n  end\nend component a\n",
        ));
        assert!(
            text.contains("use net"),
            "capabilities must survive: {text}"
        );
    }

    #[test]
    fn the_outline_is_substantially_smaller_than_the_source() {
        let src = "\
component button
  prop variant: button_variant = default
  enum button_variant
    default      class \"bg-primary text-primary-foreground hover:bg-primary/80\"
    outline      class \"border-border bg-input/30 hover:bg-input/50 hover:text-foreground\"
    secondary    class \"bg-secondary text-secondary-foreground hover:bg-secondary/80\"
    ghost        class \"hover:bg-muted hover:text-foreground dark:hover:bg-muted/50\"
  end
  view
    control
      class = \"inline-flex items-center justify-center whitespace-nowrap rounded-full\"
      text = label
    end
  end
  contract
    slot is \"button\"
  end
end component button
";
        let text = outline(&parse(src));
        assert!(
            text.len() * 2 < src.len(),
            "outline is {} bytes against {} of source — not a real saving",
            text.len(),
            src.len()
        );
    }
}
