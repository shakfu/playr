//! Effects written in Faust, in `crates/playr-dsp/faust/`, compiled to Rust
//! in `gen/` by `make faust`. The generated files are never edited; this
//! module supplies the names they use and builds each one on the heap.

pub type F32 = f32;
/// Faust 2.88.0's one-sample Rust code types its outputs `f64` whatever this
/// is, so it is `f64` to compile.
pub type FaustFloat = f64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamIndex(pub i32);

pub trait Meta {
    fn declare(&mut self, key: &str, value: &str);
}

pub trait UI<T> {
    fn open_vertical_box(&mut self, label: &str);
    fn close_box(&mut self);
    fn add_horizontal_slider(
        &mut self,
        label: &str,
        param: ParamIndex,
        init: T,
        min: T,
        max: T,
        step: T,
    );
}

/// A generated effect.
///
/// # Safety
/// Implemented only for Faust's generated structs, whose fields are all
/// `i32`, `f32` and `f64` scalars and arrays: zero bits are a valid value.
pub unsafe trait Generated {
    fn init(&mut self, sample_rate: i32);
    fn set_param(&mut self, param: ParamIndex, value: FaustFloat);
    fn control(&mut self);
    fn frame(&mut self, inputs: &[&FaustFloat], outputs: &mut [&mut FaustFloat]);
    fn instance_clear(&mut self);
    fn build(ui: &mut dyn UI<FaustFloat>);
}

macro_rules! generated {
    ($($name:ident),*) => {$(
        #[allow(
            non_snake_case,
            non_camel_case_types,
            non_upper_case_globals,
            unused_mut,
            unused_variables,
            unused_parens,
            dead_code,
            clippy::all
        )]
        pub mod $name {
            use super::{FaustFloat, Meta, ParamIndex, F32, UI};
            include!(concat!("gen/", stringify!($name), ".rs"));
        }

        unsafe impl Generated for $name::Dsp {
            fn init(&mut self, sample_rate: i32) {
                $name::Dsp::init(self, sample_rate)
            }
            fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
                $name::Dsp::set_param(self, param, value)
            }
            fn control(&mut self) {
                $name::Dsp::control(self)
            }
            fn frame(&mut self, inputs: &[&FaustFloat], outputs: &mut [&mut FaustFloat]) {
                $name::Dsp::frame(self, inputs, outputs)
            }
            fn instance_clear(&mut self) {
                $name::Dsp::instance_clear(self)
            }
            fn build(ui: &mut dyn UI<FaustFloat>) {
                $name::Dsp::build_user_interface_static(ui)
            }
        }
    )*};
}

generated!(pingpong, tape_delay, zita, jp, plate);

/// A generated effect, stereo in and out, a frame at a time.
pub struct Faust<D> {
    dsp: Box<D>,
}

impl<D: Generated> Faust<D> {
    /// Allocates; build it off the audio thread.
    pub fn new(sample_rate: u32) -> Self {
        // On the heap from the start: a delay's lines are megabytes, more
        // than a thread's stack. (`Box::new_zeroed` is newer than the MSRV.)
        let layout = std::alloc::Layout::new::<D>();
        // SAFETY: the layout is the struct's, which is not zero-sized; zero
        // bits are a valid `D` by `Generated`'s contract, and the pointer is
        // from the global allocator with `D`'s layout, as `Box` requires.
        let mut dsp = unsafe {
            let ptr = std::alloc::alloc_zeroed(layout).cast::<D>();
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            Box::from_raw(ptr)
        };
        dsp.init(sample_rate as i32);
        dsp.control();
        Faust { dsp }
    }

    /// The parameter named `label` in the `.dsp`.
    ///
    /// # Panics
    /// If it has none: the names are fixed when the `.dsp` is written.
    pub fn param(label: &str) -> ParamIndex {
        struct Find<'a>(&'a str, Option<ParamIndex>);
        impl UI<FaustFloat> for Find<'_> {
            fn open_vertical_box(&mut self, _: &str) {}
            fn close_box(&mut self) {}
            fn add_horizontal_slider(
                &mut self,
                label: &str,
                param: ParamIndex,
                _: FaustFloat,
                _: FaustFloat,
                _: FaustFloat,
                _: FaustFloat,
            ) {
                if label == self.0 {
                    self.1 = Some(param);
                }
            }
        }
        let mut find = Find(label, None);
        D::build(&mut find);
        find.1.unwrap_or_else(|| panic!("no parameter {label}"))
    }

    /// Sets a parameter. Faust does not clamp it: the caller keeps it in the
    /// `.dsp`'s range.
    pub fn set(&mut self, param: ParamIndex, value: f32) {
        self.dsp.set_param(param, f64::from(value));
        self.dsp.control();
    }

    /// Silences the tail.
    pub fn clear(&mut self) {
        self.dsp.instance_clear();
    }

    pub fn frame(&mut self, [l, r]: [f32; 2]) -> [f32; 2] {
        let (l, r) = (f64::from(l), f64::from(r));
        let (mut a, mut b) = (0.0, 0.0);
        self.dsp.frame(&[&l, &r], &mut [&mut a, &mut b]);
        [a as f32, b as f32]
    }
}
