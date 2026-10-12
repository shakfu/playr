/* ------------------------------------------------------------
license: "MIT"
name: "pingpong"
Code generated with Faust 2.88.0 (https://faust.grame.fr)
Compilation options: -lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1
------------------------------------------------------------ */

#[repr(C)]
pub struct Dsp {
	iVec0: [i32;2],
	fSampleRate: i32,
	fConst0: F32,
	fConst1: F32,
	fConst2: F32,
	fConst3: F32,
	fHslider0: FaustFloat,
	fSlow0: F32,
	fSlow1: F32,
	fRec4: [F32;2],
	fRec0: [F32;2],
	fHslider1: FaustFloat,
	fSlow2: F32,
	fSlow3: F32,
	fRec5: [F32;2],
	IOTA0: i32,
	fVec1: [F32;524288],
	fHslider2: FaustFloat,
	fSlow4: F32,
	fSlow5: F32,
	fRec6: [F32;2],
	fRec1: [F32;3],
	fRec2: [F32;2],
	fVec2: [F32;524288],
	fRec3: [F32;3],
}


pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 2;
pub const FAUST_ACTIVES: usize = 3;
pub const FAUST_PASSIVES: usize = 0;

impl Dsp {
		
	pub fn new() -> Dsp { 
		Dsp {
			iVec0: [0;2],
			fSampleRate: 0,
			fConst0: 0.0,
			fConst1: 0.0,
			fConst2: 0.0,
			fConst3: 0.0,
			fHslider0: 0.0,
			fSlow0: 0.0,
			fSlow1: 0.0,
			fRec4: [0.0;2],
			fRec0: [0.0;2],
			fHslider1: 0.0,
			fSlow2: 0.0,
			fSlow3: 0.0,
			fRec5: [0.0;2],
			IOTA0: 0,
			fVec1: [0.0;524288],
			fHslider2: 0.0,
			fSlow4: 0.0,
			fSlow5: 0.0,
			fRec6: [0.0;2],
			fRec1: [0.0;3],
			fRec2: [0.0;2],
			fVec2: [0.0;524288],
			fRec3: [0.0;3],
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("basics.lib/name", r"Faust Basic Element Library");
		m.declare("basics.lib/version", r"1.23.0");
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1");
		m.declare("delays.lib/name", r"Faust Delay Library");
		m.declare("delays.lib/version", r"1.2.0");
		m.declare("filename", r"pingpong.dsp");
		m.declare("filters.lib/lowpass0_highpass1:author", r"Julius O. Smith III");
		m.declare("filters.lib/lowpass0_highpass1:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/lowpass0_highpass1:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/lowpass:author", r"Julius O. Smith III");
		m.declare("filters.lib/lowpass:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/lowpass:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/name", r"Faust Filters Library");
		m.declare("filters.lib/tf1:author", r"Julius O. Smith III");
		m.declare("filters.lib/tf1:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/tf1:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/tf1s:author", r"Julius O. Smith III");
		m.declare("filters.lib/tf1s:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/tf1s:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/version", r"1.9.0");
		m.declare("license", r"MIT");
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"pingpong");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("routes.lib/name", r"Faust Signal Routing Library");
		m.declare("routes.lib/version", r"1.4.0");
	}

	pub fn get_sample_rate(&self) -> i32 { self.fSampleRate as i32}
	pub fn class_init(sample_rate: i32) {}
	
	pub fn static_init(&mut self,sample_rate: i32) {
	}
	pub fn instance_reset_params(&mut self) {
		self.fHslider0 = (6e+03) as FaustFloat;
		self.fHslider1 = (0.4) as FaustFloat;
		self.fHslider2 = (0.375) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		for l0 in 0..2 {
			self.iVec0[l0 as usize] = 0;
		}
		for l1 in 0..2 {
			self.fRec4[l1 as usize] = 0.0;
		}
		for l2 in 0..2 {
			self.fRec0[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fRec5[l3 as usize] = 0.0;
		}
		self.IOTA0 = 0;
		for l4 in 0..524288 {
			self.fVec1[l4 as usize] = 0.0;
		}
		for l5 in 0..2 {
			self.fRec6[l5 as usize] = 0.0;
		}
		for l6 in 0..3 {
			self.fRec1[l6 as usize] = 0.0;
		}
		for l7 in 0..2 {
			self.fRec2[l7 as usize] = 0.0;
		}
		for l8 in 0..524288 {
			self.fVec2[l8 as usize] = 0.0;
		}
		for l9 in 0..3 {
			self.fRec3[l9 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
		self.fConst1 = 3.1415927 / self.fConst0;
		self.fConst2 = F32::exp(-(5e+01 / self.fConst0));
		self.fConst3 = 1.0 - self.fConst2;
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
		ui_interface.open_vertical_box("pingpong");
		ui_interface.add_horizontal_slider("feedback", ParamIndex(0), 0.4, 0.0, 0.95, 0.001);
		ui_interface.add_horizontal_slider("time", ParamIndex(1), 0.375, 0.001, 2.0, 0.0001);
		ui_interface.add_horizontal_slider("tone", ParamIndex(2), 6e+03, 5e+02, 2e+04, 1.0);
		ui_interface.close_box();
	}
	
	pub fn get_param(&self, param: ParamIndex) -> Option<FaustFloat> {
		match param.0 {
			2 => Some(self.fHslider0),
			0 => Some(self.fHslider1),
			1 => Some(self.fHslider2),
			_ => None,
		}
	}
	
	pub fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
		match param.0 {
			2 => { self.fHslider0 = value }
			0 => { self.fHslider1 = value }
			1 => { self.fHslider2 = value }
			_ => {}
		}
	}
	
	pub fn control(&mut self) {
		// Obtaining locks on 0 static var(s)
	self.fSlow0 = (self.fHslider0) as F32;
		self.fSlow1 = self.fConst3 * self.fSlow0;
		self.fSlow2 = (self.fHslider1) as F32;
		self.fSlow3 = self.fConst3 * self.fSlow2;
		self.fSlow4 = (self.fHslider2) as F32;
		self.fSlow5 = self.fConst3 * self.fSlow4;
	}
	
	pub fn frame(&mut self, inputs: &[&FaustFloat], outputs: &mut [&mut FaustFloat]) {
		let input0 = inputs[0];
		let input1 = inputs[1];
		let (nextoutput, outputs): (&mut [&mut f64], &mut [&mut f64]) = outputs.split_at_mut(1);
		let output0: &mut FaustFloat = nextoutput[0];
		let (nextoutput, outputs): (&mut [&mut f64], &mut [&mut f64]) = outputs.split_at_mut(1);
		let output1: &mut FaustFloat = nextoutput[0];// Obtaining locks on 0 static var(s)
		
		
		// generateOneSample
		self.iVec0[0] = 1;
		let mut fTemp0: F32 = (i32::wrapping_sub(1, self.iVec0[1])) as F32;
		let mut fTemp1: F32 = self.fSlow1 + self.fConst2 * (self.fSlow0 * fTemp0 + self.fRec4[1]);
		self.fRec4[0] = (if (F32::abs(fTemp1) > 1.1754944e-38) as i32 != 0 {fTemp1} else {0.0});
		let mut fTemp2: F32 = 1.0 / F32::tan(self.fConst1 * self.fRec4[0]);
		let mut fTemp3: F32 = 1.0 - fTemp2;
		let mut fTemp4: F32 = 1.0 + fTemp2;
		let mut fTemp5: F32 = (self.fRec0[1] * fTemp3 - (self.fRec3[1] + self.fRec3[2])) / fTemp4;
		self.fRec0[0] = (if (F32::abs(-fTemp5) > 1.1754944e-38) as i32 != 0 {-fTemp5} else {0.0});
		let mut fTemp6: F32 = self.fSlow3 + self.fConst2 * (self.fSlow2 * fTemp0 + self.fRec5[1]);
		self.fRec5[0] = (if (F32::abs(fTemp6) > 1.1754944e-38) as i32 != 0 {fTemp6} else {0.0});
		let mut fTemp7: F32 = (*input0) as F32 + self.fRec0[0] * self.fRec5[0];
		self.fVec1[(self.IOTA0 & 524287) as usize] = fTemp7;
		let mut fTemp8: F32 = self.fSlow5 + self.fConst2 * (self.fSlow4 * fTemp0 + self.fRec6[1]);
		self.fRec6[0] = (if (F32::abs(fTemp8) > 1.1754944e-38) as i32 != 0 {fTemp8} else {0.0});
		let mut fTemp9: F32 = F32::max(0.0, -1.0 + self.fConst0 * self.fRec6[0]);
		let mut iTemp10: i32 = (fTemp9) as i32;
		let mut iTemp11: i32 = core::cmp::min(384001, core::cmp::max(0, iTemp10));
		let mut fTemp12: F32 = F32::floor(fTemp9);
		let mut fTemp13: F32 = 1.0 - fTemp9 + fTemp12;
		let mut fTemp14: F32 = fTemp9 - fTemp12;
		let mut iTemp15: i32 = core::cmp::min(384001, core::cmp::max(0, i32::wrapping_add(1, iTemp10)));
		let mut fTemp16: F32 = self.fVec1[((i32::wrapping_sub(self.IOTA0, iTemp11)) & 524287) as usize] * fTemp13 + fTemp14 * self.fVec1[((i32::wrapping_sub(self.IOTA0, iTemp15)) & 524287) as usize];
		self.fRec1[0] = (if (F32::abs(fTemp16) > 1.1754944e-38) as i32 != 0 {fTemp16} else {0.0});
		let mut fTemp17: F32 = (fTemp3 * self.fRec2[1] - (self.fRec1[1] + self.fRec1[2])) / fTemp4;
		self.fRec2[0] = (if (F32::abs(-fTemp17) > 1.1754944e-38) as i32 != 0 {-fTemp17} else {0.0});
		let mut fTemp18: F32 = (*input1) as F32 + self.fRec5[0] * self.fRec2[0];
		self.fVec2[(self.IOTA0 & 524287) as usize] = fTemp18;
		let mut fTemp19: F32 = fTemp13 * self.fVec2[((i32::wrapping_sub(self.IOTA0, iTemp11)) & 524287) as usize] + fTemp14 * self.fVec2[((i32::wrapping_sub(self.IOTA0, iTemp15)) & 524287) as usize];
		self.fRec3[0] = (if (F32::abs(fTemp19) > 1.1754944e-38) as i32 != 0 {fTemp19} else {0.0});
		*output0 = (self.fRec3[1]) as FaustFloat;
		*output1 = (self.fRec1[1]) as FaustFloat;
		self.iVec0[1] = self.iVec0[0];
		self.fRec4[1] = self.fRec4[0];
		self.fRec0[1] = self.fRec0[0];
		self.fRec5[1] = self.fRec5[0];
		self.IOTA0 = i32::wrapping_add(self.IOTA0, 1);
		self.fRec6[1] = self.fRec6[0];
		self.fRec1[2] = self.fRec1[1];
		self.fRec1[1] = self.fRec1[0];
		self.fRec2[1] = self.fRec2[0];
		self.fRec3[2] = self.fRec3[1];
		self.fRec3[1] = self.fRec3[0];
		
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

