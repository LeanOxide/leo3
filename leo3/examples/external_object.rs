//! Example: External Objects (LeanExternal / ExternalClass).
//!
//! Demonstrates wrapping Rust structs as Lean external objects,
//! borrowing, mutating, type checking, and conversion traits.
//!
//! Run with:
//! ```bash
//! cargo run --example external_object
//! ```

use leo3::external::{ExternalClass, LeanExternal};
use leo3::prelude::*;

#[derive(Clone, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

impl ExternalClass for Point {
    fn class_name() -> &'static str {
        "Point"
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Label {
    text: String,
}

impl ExternalClass for Label {
    fn class_name() -> &'static str {
        "Label"
    }
}

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        println!("=== External Object Example ===\n");

        println!("1. Create external object:");
        let point = LeanExternal::new(lean, Point { x: 3.0, y: 4.0 })?;
        println!("   Created: {:?}", point.get_ref());

        println!("\n2. Borrow inner value:");
        let p: &Point = point.borrow();
        println!(
            "   Distance from origin: {}",
            (p.x * p.x + p.y * p.y).sqrt()
        );

        println!("\n3. Mutable access (try_get_mut):");
        let mut point = point;
        if let Some(p) = point.try_get_mut() {
            p.x *= 2.0;
            p.y *= 2.0;
        }
        println!("   After scaling: {:?}", point.get_ref());

        println!("\n4. Type checking:");
        println!("   is_type::<Point>: {}", point.is_type::<Point>());
        println!("   is_type::<Label>: {}", point.is_type::<Label>());

        println!("\n5. IntoLean / FromLean conversion:");
        let original = Point { x: 1.0, y: 2.0 };
        let external: LeanExternal<'_, Point> = original.clone().into_lean(lean)?;
        println!("   IntoLean: {:?}", external.get_ref());
        let recovered = Point::from_lean(&external.cast())?;
        println!("   FromLean: {:?}", recovered);
        assert_eq!(original, recovered);

        println!("\n6. try_take_inner (move out):");
        let mut external = LeanExternal::new(lean, Point { x: 9.0, y: 9.0 })?;
        if let Some(inner) = external.try_take_inner() {
            println!("   Took ownership: {:?}", inner);
        }

        println!("\n7. Multiple external types:");
        let label = LeanExternal::new(
            lean,
            Label {
                text: "origin".into(),
            },
        )?;
        println!("   Label: {:?}", label.get_ref());
        println!("   label.is_type::<Label>: {}", label.is_type::<Label>());
        println!("   label.is_type::<Point>: {}", label.is_type::<Point>());

        println!("\n=== All external object operations completed successfully! ===");
        Ok(())
    })
}
