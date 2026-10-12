/* ------------------------------------------------------------
name: "plate"
Code generated with Faust 2.88.0 (https://faust.grame.fr)
Compilation options: -lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1
------------------------------------------------------------ */

#[repr(C)]
pub struct Dsp {
	IOTA0: i32,
	fVec0: [F32;1024],
	fRec0: [F32;2],
	fVec1: [F32;1024],
	fRec1: [F32;2],
	fHslider0: FaustFloat,
	fSlow0: F32,
	fSlow1: F32,
	fVec2: [F32;8192],
	fRec4: [F32;2],
	fSampleRate: i32,
	fConst0: F32,
	fHslider1: FaustFloat,
	fSlow2: F32,
	fVec3: [F32;2048],
	fRec6: [F32;2],
	fVec4: [F32;8192],
	fRec7: [F32;2],
	fVec5: [F32;4096],
	fRec9: [F32;2],
	fRec15: [F32;3],
	fVec6: [F32;256],
	fRec14: [F32;2],
	fVec7: [F32;128],
	fRec13: [F32;2],
	fVec8: [F32;512],
	fRec12: [F32;2],
	fVec9: [F32;512],
	fRec16: [F32;2],
	fVec10: [F32;4096],
	fRec10: [F32;2],
	fVec11: [F32;2048],
	fRec11: [F32;2],
}


pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 2;
pub const FAUST_ACTIVES: usize = 2;
pub const FAUST_PASSIVES: usize = 0;

impl Dsp {
		
	pub fn new() -> Dsp { 
		Dsp {
			IOTA0: 0,
			fVec0: [0.0;1024],
			fRec0: [0.0;2],
			fVec1: [0.0;1024],
			fRec1: [0.0;2],
			fHslider0: 0.0,
			fSlow0: 0.0,
			fSlow1: 0.0,
			fVec2: [0.0;8192],
			fRec4: [0.0;2],
			fSampleRate: 0,
			fConst0: 0.0,
			fHslider1: 0.0,
			fSlow2: 0.0,
			fVec3: [0.0;2048],
			fRec6: [0.0;2],
			fVec4: [0.0;8192],
			fRec7: [0.0;2],
			fVec5: [0.0;4096],
			fRec9: [0.0;2],
			fRec15: [0.0;3],
			fVec6: [0.0;256],
			fRec14: [0.0;2],
			fVec7: [0.0;128],
			fRec13: [0.0;2],
			fVec8: [0.0;512],
			fRec12: [0.0;2],
			fVec9: [0.0;512],
			fRec16: [0.0;2],
			fVec10: [0.0;4096],
			fRec10: [0.0;2],
			fVec11: [0.0;2048],
			fRec11: [0.0;2],
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("basics.lib/name", r"Faust Basic Element Library");
		m.declare("basics.lib/version", r"1.23.0");
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1");
		m.declare("filename", r"plate.dsp");
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"plate");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("reverbs.lib/dattorro_rev:author", r"Jakob Zerbian");
		m.declare("reverbs.lib/dattorro_rev:licence", r"LicenseRef-STK-4.3");
		m.declare("reverbs.lib/name", r"Faust Reverb Library");
		m.declare("reverbs.lib/version", r"1.5.1");
		m.declare("routes.lib/name", r"Faust Signal Routing Library");
		m.declare("routes.lib/version", r"1.4.0");
		m.declare("signals.lib/name", r"Faust Routing Library");
		m.declare("signals.lib/version", r"1.7.0");
	}

	pub fn get_sample_rate(&self) -> i32 { self.fSampleRate as i32}
	pub fn class_init(sample_rate: i32) {}
	
	pub fn static_init(&mut self,sample_rate: i32) {
	}
	pub fn instance_reset_params(&mut self) {
		self.fHslider0 = (0.3) as FaustFloat;
		self.fHslider1 = (2.0) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		self.IOTA0 = 0;
		for l0 in 0..1024 {
			self.fVec0[l0 as usize] = 0.0;
		}
		for l1 in 0..2 {
			self.fRec0[l1 as usize] = 0.0;
		}
		for l2 in 0..1024 {
			self.fVec1[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fRec1[l3 as usize] = 0.0;
		}
		for l4 in 0..8192 {
			self.fVec2[l4 as usize] = 0.0;
		}
		for l5 in 0..2 {
			self.fRec4[l5 as usize] = 0.0;
		}
		for l6 in 0..2048 {
			self.fVec3[l6 as usize] = 0.0;
		}
		for l7 in 0..2 {
			self.fRec6[l7 as usize] = 0.0;
		}
		for l8 in 0..8192 {
			self.fVec4[l8 as usize] = 0.0;
		}
		for l9 in 0..2 {
			self.fRec7[l9 as usize] = 0.0;
		}
		for l10 in 0..4096 {
			self.fVec5[l10 as usize] = 0.0;
		}
		for l11 in 0..2 {
			self.fRec9[l11 as usize] = 0.0;
		}
		for l12 in 0..3 {
			self.fRec15[l12 as usize] = 0.0;
		}
		for l13 in 0..256 {
			self.fVec6[l13 as usize] = 0.0;
		}
		for l14 in 0..2 {
			self.fRec14[l14 as usize] = 0.0;
		}
		for l15 in 0..128 {
			self.fVec7[l15 as usize] = 0.0;
		}
		for l16 in 0..2 {
			self.fRec13[l16 as usize] = 0.0;
		}
		for l17 in 0..512 {
			self.fVec8[l17 as usize] = 0.0;
		}
		for l18 in 0..2 {
			self.fRec12[l18 as usize] = 0.0;
		}
		for l19 in 0..512 {
			self.fVec9[l19 as usize] = 0.0;
		}
		for l20 in 0..2 {
			self.fRec16[l20 as usize] = 0.0;
		}
		for l21 in 0..4096 {
			self.fVec10[l21 as usize] = 0.0;
		}
		for l22 in 0..2 {
			self.fRec10[l22 as usize] = 0.0;
		}
		for l23 in 0..2048 {
			self.fVec11[l23 as usize] = 0.0;
		}
		for l24 in 0..2 {
			self.fRec11[l24 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = 1.74e+04 / F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
	}
	pub fn instance_init(&mut self, sample_rate: i32) {
		self.static_init(sample_rate);
		self.instance_constants(sample_rate);
		self.instance_reset_params();
		self.instance_clear();
	}
	pub fn init(&mut self, sample_rate: i32) {
		self.instance_init(sample_rate);
	}
	
	pub fn build_user_interface(&self, ui_interface: &mut dyn UI<FaustFloat>) {
		Self::build_user_interface_static(ui_interface);
	}
	
	pub fn build_user_interface_static(ui_interface: &mut dyn UI<FaustFloat>) {
		ui_interface.open_vertical_box("plate");
		ui_interface.add_horizontal_slider("damp", ParamIndex(0), 0.3, 0.0, 1.0, 0.001);
		ui_interface.add_horizontal_slider("decay", ParamIndex(1), 2.0, 0.1, 2e+01, 0.01);
		ui_interface.close_box();
	}
	
	pub fn get_param(&self, param: ParamIndex) -> Option<FaustFloat> {
		match param.0 {
			0 => Some(self.fHslider0),
			1 => Some(self.fHslider1),
			_ => None,
		}
	}
	
	pub fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
		match param.0 {
			0 => { self.fHslider0 = value }
			1 => { self.fHslider1 = value }
			_ => {}
		}
	}
	
	pub fn control(&mut self) {
		// Obtaining locks on 0 static var(s)
	self.fSlow0 = 0.9 * (self.fHslider0) as F32;
		self.fSlow1 = 1.0 - self.fSlow0;
		self.fSlow2 = F32::min(0.9999, F32::powf(1e+01, -(self.fConst0 / (self.fHslider1) as F32)));
	}
	
	pub fn frame(&mut self, inputs: &[&FaustFloat], outputs: &mut [&mut FaustFloat]) {
		let input0 = inputs[0];
		let input1 = inputs[1];
		let (nextoutput, outputs): (&mut [&mut f64], &mut [&mut f64]) = outputs.split_at_mut(1);
		let output0: &mut FaustFloat = nextoutput[0];
		let (nextoutput, outputs): (&mut [&mut f64], &mut [&mut f64]) = outputs.split_at_mut(1);
		let output1: &mut FaustFloat = nextoutput[0];// Obtaining locks on 0 static var(s)
		
		
		// generateOneSample
		let mut fTemp0: F32 = 0.7 * self.fRec0[1] + self.fRec10[1];
		self.fVec0[(self.IOTA0 & 1023) as usize] = fTemp0;
		let mut fTemp1: F32 = self.fVec0[((i32::wrapping_sub(self.IOTA0, 672)) & 1023) as usize];
		self.fRec0[0] = (if (F32::abs(fTemp1) > 1.1754944e-38) as i32 != 0 {fTemp1} else {0.0});
		let mut fTemp2: F32 = 0.7 * self.fRec1[1] + self.fRec11[1];
		self.fVec1[(self.IOTA0 & 1023) as usize] = fTemp2;
		let mut fTemp3: F32 = self.fVec1[((i32::wrapping_sub(self.IOTA0, 908)) & 1023) as usize];
		self.fRec1[0] = (if (F32::abs(fTemp3) > 1.1754944e-38) as i32 != 0 {fTemp3} else {0.0});
		let mut fTemp4: F32 = 0.7 * fTemp0;
		let mut fRec2: F32 = (if (F32::abs(-fTemp4) > 1.1754944e-38) as i32 != 0 {-fTemp4} else {0.0});
		let mut fTemp5: F32 = 0.7 * fTemp2;
		let mut fRec3: F32 = (if (F32::abs(-fTemp5) > 1.1754944e-38) as i32 != 0 {-fTemp5} else {0.0});
		self.fVec2[(self.IOTA0 & 8191) as usize] = self.fRec0[1] + fRec2;
		let mut fTemp6: F32 = self.fSlow0 * self.fRec4[1] + self.fSlow1 * self.fVec2[((i32::wrapping_sub(self.IOTA0, 4453)) & 8191) as usize];
		self.fRec4[0] = (if (F32::abs(fTemp6) > 1.1754944e-38) as i32 != 0 {fTemp6} else {0.0});
		let mut fTemp7: F32 = self.fSlow2 * self.fRec4[0] - 0.5 * self.fRec6[1];
		self.fVec3[(self.IOTA0 & 2047) as usize] = fTemp7;
		let mut fTemp8: F32 = 0.5 * fTemp7;
		let mut fRec5: F32 = (if (F32::abs(fTemp8) > 1.1754944e-38) as i32 != 0 {fTemp8} else {0.0});
		let mut fTemp9: F32 = self.fVec3[((i32::wrapping_sub(self.IOTA0, 1800)) & 2047) as usize];
		self.fRec6[0] = (if (F32::abs(fTemp9) > 1.1754944e-38) as i32 != 0 {fTemp9} else {0.0});
		self.fVec4[(self.IOTA0 & 8191) as usize] = self.fRec1[1] + fRec3;
		let mut fTemp10: F32 = self.fSlow0 * self.fRec7[1] + self.fSlow1 * self.fVec4[((i32::wrapping_sub(self.IOTA0, 4217)) & 8191) as usize];
		self.fRec7[0] = (if (F32::abs(fTemp10) > 1.1754944e-38) as i32 != 0 {fTemp10} else {0.0});
		let mut fTemp11: F32 = self.fSlow2 * self.fRec7[0] - 0.5 * self.fRec9[1];
		self.fVec5[(self.IOTA0 & 4095) as usize] = fTemp11;
		let mut fTemp12: F32 = 0.5 * fTemp11;
		let mut fRec8: F32 = (if (F32::abs(fTemp12) > 1.1754944e-38) as i32 != 0 {fTemp12} else {0.0});
		let mut fTemp13: F32 = self.fVec5[((i32::wrapping_sub(self.IOTA0, 2656)) & 4095) as usize];
		self.fRec9[0] = (if (F32::abs(fTemp13) > 1.1754944e-38) as i32 != 0 {fTemp13} else {0.0});
		let mut fTemp14: F32 = 0.49975 * ((*input0) as F32 + (*input1) as F32) + 0.0005 * self.fRec15[2];
		self.fRec15[0] = (if (F32::abs(fTemp14) > 1.1754944e-38) as i32 != 0 {fTemp14} else {0.0});
		let mut fTemp15: F32 = self.fRec15[0] - 0.75 * self.fRec14[1];
		self.fVec6[(self.IOTA0 & 255) as usize] = fTemp15;
		let mut fTemp16: F32 = self.fVec6[((i32::wrapping_sub(self.IOTA0, 142)) & 255) as usize];
		self.fRec14[0] = (if (F32::abs(fTemp16) > 1.1754944e-38) as i32 != 0 {fTemp16} else {0.0});
		let mut fTemp17: F32 = self.fRec14[1] - 0.75 * (self.fRec13[1] - fTemp15);
		self.fVec7[(self.IOTA0 & 127) as usize] = fTemp17;
		let mut fTemp18: F32 = self.fVec7[((i32::wrapping_sub(self.IOTA0, 107)) & 127) as usize];
		self.fRec13[0] = (if (F32::abs(fTemp18) > 1.1754944e-38) as i32 != 0 {fTemp18} else {0.0});
		let mut fTemp19: F32 = self.fRec13[1] + 0.75 * fTemp17 - 0.625 * self.fRec12[1];
		self.fVec8[(self.IOTA0 & 511) as usize] = fTemp19;
		let mut fTemp20: F32 = self.fVec8[((i32::wrapping_sub(self.IOTA0, 379)) & 511) as usize];
		self.fRec12[0] = (if (F32::abs(fTemp20) > 1.1754944e-38) as i32 != 0 {fTemp20} else {0.0});
		let mut fTemp21: F32 = self.fRec12[1] - 0.625 * (self.fRec16[1] - fTemp19);
		self.fVec9[(self.IOTA0 & 511) as usize] = fTemp21;
		let mut fTemp22: F32 = self.fVec9[((i32::wrapping_sub(self.IOTA0, 277)) & 511) as usize];
		self.fRec16[0] = (if (F32::abs(fTemp22) > 1.1754944e-38) as i32 != 0 {fTemp22} else {0.0});
		let mut fTemp23: F32 = 0.625 * fTemp21;
		self.fVec10[(self.IOTA0 & 4095) as usize] = self.fRec9[1] + fRec8;
		let mut fTemp24: F32 = fTemp23 + self.fSlow2 * self.fVec10[((i32::wrapping_sub(self.IOTA0, 2656)) & 4095) as usize] + self.fRec16[1];
		self.fRec10[0] = (if (F32::abs(fTemp24) > 1.1754944e-38) as i32 != 0 {fTemp24} else {0.0});
		self.fVec11[(self.IOTA0 & 2047) as usize] = self.fRec6[1] + fRec5;
		let mut fTemp25: F32 = self.fRec16[1] + fTemp23 + self.fSlow2 * self.fVec11[((i32::wrapping_sub(self.IOTA0, 1800)) & 2047) as usize];
		self.fRec11[0] = (if (F32::abs(fTemp25) > 1.1754944e-38) as i32 != 0 {fTemp25} else {0.0});
		*output0 = (self.fRec10[0]) as FaustFloat;
		*output1 = (self.fRec11[0]) as FaustFloat;
		self.IOTA0 = i32::wrapping_add(self.IOTA0, 1);
		self.fRec0[1] = self.fRec0[0];
		self.fRec1[1] = self.fRec1[0];
		self.fRec4[1] = self.fRec4[0];
		self.fRec6[1] = self.fRec6[0];
		self.fRec7[1] = self.fRec7[0];
		self.fRec9[1] = self.fRec9[0];
		self.fRec15[2] = self.fRec15[1];
		self.fRec15[1] = self.fRec15[0];
		self.fRec14[1] = self.fRec14[0];
		self.fRec13[1] = self.fRec13[0];
		self.fRec12[1] = self.fRec12[0];
		self.fRec16[1] = self.fRec16[0];
		self.fRec10[1] = self.fRec10[0];
		self.fRec11[1] = self.fRec11[0];
		
	}
	
}

#[cfg(not(target_arch = "wasm32"))] // Compile ffi bindings only on non-wasm targets
mod ffi {
	use core::ffi::c_float;
	// Conditionally compile the link attribute only on non-Windows platforms
	#[cfg_attr(not(target_os = "windows"), link(name = "m"))]
	unsafe extern "C" {
		pub fn remainderf(from: c_float, to: c_float) -> c_float;
		pub fn rintf(val: c_float) -> c_float;
	}
}
fn remainderf(from: f32, to: f32) -> f32 {
	#[cfg(not(target_arch = "wasm32"))] // non-wasm targets use ffi bindings
	unsafe { ffi::remainderf(from, to) }
	#[cfg(target_arch = "wasm32")] // wasm relies on libm
	libm::remainderf(from, to)
}
fn rintf(val: f32) -> f32 {
	#[cfg(not(target_arch = "wasm32"))] // non-wasm targets use ffi bindings
	unsafe { ffi::rintf(val) }
	#[cfg(target_arch = "wasm32")] // wasm relies on libm
	libm::rintf(val)
}

