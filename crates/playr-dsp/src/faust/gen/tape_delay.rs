/* ------------------------------------------------------------
license: "MIT"
name: "tape_delay"
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
	fRec2: [F32;2],
	fRec0: [F32;2],
	fHslider1: FaustFloat,
	fSlow2: F32,
	fSlow3: F32,
	fRec3: [F32;2],
	fVec1: [F32;2],
	IOTA0: i32,
	fVec2: [F32;524288],
	fHslider2: FaustFloat,
	fSlow4: F32,
	fSlow5: F32,
	fRec4: [F32;2],
	fHslider3: FaustFloat,
	fSlow6: F32,
	fSlow7: F32,
	fRec5: [F32;2],
	ftbl0DspSIG0: [F32;65536],
	fConst4: F32,
	fRec7: [F32;2],
	fConst5: F32,
	fRec8: [F32;2],
	fConst6: F32,
	fConst7: F32,
	fConst8: F32,
	fConst9: F32,
	iRec10: [i32;2],
	fConst10: F32,
	fConst11: F32,
	fConst12: F32,
	fRec9: [F32;3],
	fRec1: [F32;3],
	fRec11: [F32;2],
	fVec4: [F32;2],
	fVec5: [F32;524288],
	ftbl1DspSIG1: [F32;65536],
	fRec12: [F32;3],
	iVec3: [i32;2],
	iRec6: [i32;2],
	iVec6: [i32;2],
	iRec13: [i32;2],
}


fn Dsp_faustpower3_f(value: F32) -> F32 {
	return value * value * value;
}
fn Dsp_faustpower2_f(value: F32) -> F32 {
	return value * value;
}
pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 2;
pub const FAUST_ACTIVES: usize = 4;
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
			fRec2: [0.0;2],
			fRec0: [0.0;2],
			fHslider1: 0.0,
			fSlow2: 0.0,
			fSlow3: 0.0,
			fRec3: [0.0;2],
			fVec1: [0.0;2],
			IOTA0: 0,
			fVec2: [0.0;524288],
			fHslider2: 0.0,
			fSlow4: 0.0,
			fSlow5: 0.0,
			fRec4: [0.0;2],
			fHslider3: 0.0,
			fSlow6: 0.0,
			fSlow7: 0.0,
			fRec5: [0.0;2],
			ftbl0DspSIG0: [0.0;65536],
			fConst4: 0.0,
			fRec7: [0.0;2],
			fConst5: 0.0,
			fRec8: [0.0;2],
			fConst6: 0.0,
			fConst7: 0.0,
			fConst8: 0.0,
			fConst9: 0.0,
			iRec10: [0;2],
			fConst10: 0.0,
			fConst11: 0.0,
			fConst12: 0.0,
			fRec9: [0.0;3],
			fRec1: [0.0;3],
			fRec11: [0.0;2],
			fVec4: [0.0;2],
			fVec5: [0.0;524288],
			ftbl1DspSIG1: [0.0;65536],
			fRec12: [0.0;3],
			iVec3: [0;2],
			iRec6: [0;2],
			iVec6: [0;2],
			iRec13: [0;2],
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("aanl.lib/ADAA1:author", r"Dario Sanfilippo");
		m.declare("aanl.lib/ADAA1:copyright", r"Copyright (C) 2021 Dario Sanfilippo     <sanfilippo.dario@gmail.com>");
		m.declare("aanl.lib/ADAA1:license", r"MIT");
		m.declare("aanl.lib/name", r"Faust Antialiased Nonlinearities");
		m.declare("aanl.lib/softclipQuadratic1:author", r"David Braun");
		m.declare("aanl.lib/softclipQuadratic1:copyright", r"Copyright (C) 2024 David Braun");
		m.declare("aanl.lib/softclipQuadratic1:license", r"MIT");
		m.declare("aanl.lib/version", r"1.5.0");
		m.declare("basics.lib/ifNc:author", r"Oleg Nesterov");
		m.declare("basics.lib/ifNc:copyright", r"Copyright (C) 2023 Oleg Nesterov <oleg@redhat.com>");
		m.declare("basics.lib/ifNc:license", r"LicenseRef-STK-4.3");
		m.declare("basics.lib/ifNcNo:author", r"Oleg Nesterov");
		m.declare("basics.lib/ifNcNo:copyright", r"Copyright (C) 2023 Oleg Nesterov <oleg@redhat.com>");
		m.declare("basics.lib/ifNcNo:license", r"LicenseRef-STK-4.3");
		m.declare("basics.lib/name", r"Faust Basic Element Library");
		m.declare("basics.lib/version", r"1.23.0");
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1");
		m.declare("delays.lib/name", r"Faust Delay Library");
		m.declare("delays.lib/version", r"1.2.0");
		m.declare("filename", r"tape_delay.dsp");
		m.declare("filters.lib/fir:author", r"Julius O. Smith III");
		m.declare("filters.lib/fir:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/fir:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/iir:author", r"Julius O. Smith III");
		m.declare("filters.lib/iir:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/iir:license", r"LicenseRef-STK-4.3");
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
		m.declare("filters.lib/tf2:author", r"Julius O. Smith III");
		m.declare("filters.lib/tf2:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/tf2:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/tf2s:author", r"Julius O. Smith III");
		m.declare("filters.lib/tf2s:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/tf2s:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/version", r"1.9.0");
		m.declare("license", r"MIT");
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"tape_delay");
		m.declare("noises.lib/name", r"Faust Noise Generator Library");
		m.declare("noises.lib/version", r"1.6.0");
		m.declare("oscillators.lib/name", r"Faust Oscillator Library");
		m.declare("oscillators.lib/version", r"1.8.0");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("signals.lib/name", r"Faust Routing Library");
		m.declare("signals.lib/version", r"1.7.0");
	}

	pub fn get_sample_rate(&self) -> i32 { self.fSampleRate as i32}
	pub fn class_init(sample_rate: i32) {}
	
	pub fn static_init(&mut self,sample_rate: i32) {
		for l8 in 0..2 {
			self.iVec3[l8 as usize] = 0;
		}
		for l9 in 0..2 {
			self.iRec6[l9 as usize] = 0;
		}
		for i1_re0 in 0..65536 {
			self.iVec3[0] = 1;
			self.iRec6[0] = (i32::wrapping_add(self.iVec3[1], self.iRec6[1])) % 65536;
			self.ftbl0DspSIG0[i1_re0 as usize] = F32::sin(9.58738e-05 * (self.iRec6[0]) as F32);
			self.iVec3[1] = self.iVec3[0];
			self.iRec6[1] = self.iRec6[0];
		}
		for l18 in 0..2 {
			self.iVec6[l18 as usize] = 0;
		}
		for l19 in 0..2 {
			self.iRec13[l19 as usize] = 0;
		}
		for i2_re0 in 0..65536 {
			self.iVec6[0] = 1;
			self.iRec13[0] = (i32::wrapping_add(self.iVec6[1], self.iRec13[1])) % 65536;
			self.ftbl1DspSIG1[i2_re0 as usize] = F32::cos(9.58738e-05 * (self.iRec13[0]) as F32);
			self.iVec6[1] = self.iVec6[0];
			self.iRec13[1] = self.iRec13[0];
		}
	}
	pub fn instance_reset_params(&mut self) {
		self.fHslider0 = (6e+03) as FaustFloat;
		self.fHslider1 = (0.4) as FaustFloat;
		self.fHslider2 = (0.375) as FaustFloat;
		self.fHslider3 = (0.3) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		for l0 in 0..2 {
			self.iVec0[l0 as usize] = 0;
		}
		for l1 in 0..2 {
			self.fRec2[l1 as usize] = 0.0;
		}
		for l2 in 0..2 {
			self.fRec0[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fRec3[l3 as usize] = 0.0;
		}
		for l4 in 0..2 {
			self.fVec1[l4 as usize] = 0.0;
		}
		self.IOTA0 = 0;
		for l5 in 0..524288 {
			self.fVec2[l5 as usize] = 0.0;
		}
		for l6 in 0..2 {
			self.fRec4[l6 as usize] = 0.0;
		}
		for l7 in 0..2 {
			self.fRec5[l7 as usize] = 0.0;
		}
		for l10 in 0..2 {
			self.fRec7[l10 as usize] = 0.0;
		}
		for l11 in 0..2 {
			self.fRec8[l11 as usize] = 0.0;
		}
		for l12 in 0..2 {
			self.iRec10[l12 as usize] = 0;
		}
		for l13 in 0..3 {
			self.fRec9[l13 as usize] = 0.0;
		}
		for l14 in 0..3 {
			self.fRec1[l14 as usize] = 0.0;
		}
		for l15 in 0..2 {
			self.fRec11[l15 as usize] = 0.0;
		}
		for l16 in 0..2 {
			self.fVec4[l16 as usize] = 0.0;
		}
		for l17 in 0..524288 {
			self.fVec5[l17 as usize] = 0.0;
		}
		for l20 in 0..3 {
			self.fRec12[l20 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
		self.fConst1 = 3.1415927 / self.fConst0;
		self.fConst2 = F32::exp(-(5e+01 / self.fConst0));
		self.fConst3 = 1.0 - self.fConst2;
		self.fConst4 = 0.6 / self.fConst0;
		self.fConst5 = 9.0 / self.fConst0;
		self.fConst6 = F32::tan(9.424778 / self.fConst0);
		self.fConst7 = 1.0 / self.fConst6;
		self.fConst8 = (self.fConst7 + 1.4142135) / self.fConst6 + 1.0;
		self.fConst9 = 0.0003 / self.fConst8;
		self.fConst10 = 1.0 / self.fConst8;
		self.fConst11 = (self.fConst7 + -1.4142135) / self.fConst6 + 1.0;
		self.fConst12 = 2.0 * (1.0 - 1.0 / Dsp_faustpower2_f(self.fConst6));
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
		ui_interface.open_vertical_box("tape_delay");
		ui_interface.add_horizontal_slider("feedback", ParamIndex(0), 0.4, 0.0, 0.95, 0.001);
		ui_interface.add_horizontal_slider("time", ParamIndex(1), 0.375, 0.001, 2.0, 0.0001);
		ui_interface.add_horizontal_slider("tone", ParamIndex(2), 6e+03, 5e+02, 2e+04, 1.0);
		ui_interface.add_horizontal_slider("wow", ParamIndex(3), 0.3, 0.0, 1.0, 0.001);
		ui_interface.close_box();
	}
	
	pub fn get_param(&self, param: ParamIndex) -> Option<FaustFloat> {
		match param.0 {
			2 => Some(self.fHslider0),
			0 => Some(self.fHslider1),
			1 => Some(self.fHslider2),
			3 => Some(self.fHslider3),
			_ => None,
		}
	}
	
	pub fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
		match param.0 {
			2 => { self.fHslider0 = value }
			0 => { self.fHslider1 = value }
			1 => { self.fHslider2 = value }
			3 => { self.fHslider3 = value }
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
		self.fSlow6 = (self.fHslider3) as F32;
		self.fSlow7 = self.fConst3 * self.fSlow6;
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
		let mut iTemp0: i32 = i32::wrapping_sub(1, self.iVec0[1]);
		let mut fTemp1: F32 = (iTemp0) as F32;
		let mut fTemp2: F32 = self.fSlow1 + self.fConst2 * (self.fSlow0 * fTemp1 + self.fRec2[1]);
		self.fRec2[0] = (if (F32::abs(fTemp2) > 1.1754944e-38) as i32 != 0 {fTemp2} else {0.0});
		let mut fTemp3: F32 = 1.0 / F32::tan(self.fConst1 * self.fRec2[0]);
		let mut fTemp4: F32 = 1.0 - fTemp3;
		let mut fTemp5: F32 = 1.0 + fTemp3;
		let mut fTemp6: F32 = (self.fRec0[1] * fTemp4 - (self.fRec1[1] + self.fRec1[2])) / fTemp5;
		self.fRec0[0] = (if (F32::abs(-fTemp6) > 1.1754944e-38) as i32 != 0 {-fTemp6} else {0.0});
		let mut fTemp7: F32 = self.fSlow3 + self.fConst2 * (self.fSlow2 * fTemp1 + self.fRec3[1]);
		self.fRec3[0] = (if (F32::abs(fTemp7) > 1.1754944e-38) as i32 != 0 {fTemp7} else {0.0});
		let mut fTemp8: F32 = self.fRec0[0] * self.fRec3[0];
		self.fVec1[0] = fTemp8;
		let mut fTemp9: F32 = fTemp8 - self.fVec1[1];
		let mut fTemp10: F32 = 0.5 * fTemp8;
		let mut fTemp11: F32 = F32::abs(fTemp10);
		let mut fTemp12: F32 = Dsp_faustpower2_f(fTemp10);
		let mut fTemp13: F32 = 0.5 * self.fVec1[1];
		let mut fTemp14: F32 = F32::abs(fTemp13);
		let mut fTemp15: F32 = Dsp_faustpower2_f(fTemp13);
		let mut fTemp16: F32 = fTemp8 + self.fVec1[1];
		let mut fTemp17: F32 = 0.25 * fTemp16;
		let mut fTemp18: F32 = F32::abs(fTemp17);
		let mut fTemp19: F32 = (((fTemp17 > 0.0) as i32) - ((fTemp17 < 0.0) as i32)) as F32;
		let mut fTemp20: F32 = (*input0) as F32 + (if (F32::abs(0.5 * fTemp9) <= 0.001) as i32 != 0 {(if (fTemp18 < 0.33333334) as i32 != 0 {0.5 * fTemp16} else {(if (fTemp18 < 0.6666667) as i32 != 0 {0.33333334 * fTemp19 * (3.0 - Dsp_faustpower2_f(2.0 - 3.0 * fTemp18))} else {fTemp19})})} else {2.0 * (((if (fTemp11 < 0.33333334) as i32 != 0 {fTemp12} else {(if (fTemp11 < 0.6666667) as i32 != 0 {0.037037037 - (0.33333334 * fTemp11 + Dsp_faustpower3_f(fTemp11)) + 2.0 * fTemp12} else {-0.25925925 + fTemp11})}) - (if (fTemp14 < 0.33333334) as i32 != 0 {fTemp15} else {(if (fTemp14 < 0.6666667) as i32 != 0 {0.037037037 - (0.33333334 * fTemp14 + Dsp_faustpower3_f(fTemp14)) + 2.0 * fTemp15} else {-0.25925925 + fTemp14})})) / fTemp9)});
		self.fVec2[(self.IOTA0 & 524287) as usize] = fTemp20;
		let mut fTemp21: F32 = self.fSlow5 + self.fConst2 * (self.fSlow4 * fTemp1 + self.fRec4[1]);
		self.fRec4[0] = (if (F32::abs(fTemp21) > 1.1754944e-38) as i32 != 0 {fTemp21} else {0.0});
		let mut fTemp22: F32 = self.fSlow7 + self.fConst2 * (self.fSlow6 * fTemp1 + self.fRec5[1]);
		self.fRec5[0] = (if (F32::abs(fTemp22) > 1.1754944e-38) as i32 != 0 {fTemp22} else {0.0});
		let mut fTemp23: F32 = (if iTemp0 != 0 {0.0} else {self.fConst4 + self.fRec7[1]});
		let mut fTemp24: F32 = fTemp23 - F32::floor(fTemp23);
		self.fRec7[0] = (if (F32::abs(fTemp24) > 1.1754944e-38) as i32 != 0 {fTemp24} else {0.0});
		let mut iTemp25: i32 = core::cmp::max(0, core::cmp::min((65536.0 * self.fRec7[0]) as i32, 65535));
		let mut fTemp26: F32 = (if iTemp0 != 0 {0.0} else {self.fConst5 + self.fRec8[1]});
		let mut fTemp27: F32 = fTemp26 - F32::floor(fTemp26);
		self.fRec8[0] = (if (F32::abs(fTemp27) > 1.1754944e-38) as i32 != 0 {fTemp27} else {0.0});
		let mut iTemp28: i32 = core::cmp::max(0, core::cmp::min((65536.0 * self.fRec8[0]) as i32, 65535));
		self.iRec10[0] = i32::wrapping_add(12345, i32::wrapping_mul(1103515245, self.iRec10[1]));
		let mut fTemp29: F32 = 4.656613e-10 * (self.iRec10[0]) as F32 - self.fConst10 * (self.fConst11 * self.fRec9[2] + self.fConst12 * self.fRec9[1]);
		self.fRec9[0] = (if (F32::abs(fTemp29) > 1.1754944e-38) as i32 != 0 {fTemp29} else {0.0});
		let mut fTemp30: F32 = self.fConst9 * (self.fRec9[2] + self.fRec9[0] + 2.0 * self.fRec9[1]);
		let mut fTemp31: F32 = F32::max(0.0, -1.0 + self.fConst0 * (self.fRec4[0] + self.fRec5[0] * (0.0013 * self.ftbl0DspSIG0[iTemp25 as usize] + 1.77e-05 * self.ftbl0DspSIG0[iTemp28 as usize] + fTemp30)));
		let mut iTemp32: i32 = (fTemp31) as i32;
		let mut fTemp33: F32 = F32::floor(fTemp31);
		let mut fTemp34: F32 = self.fVec2[((i32::wrapping_sub(self.IOTA0, core::cmp::min(384001, core::cmp::max(0, iTemp32)))) & 524287) as usize] * (1.0 - fTemp31 + fTemp33) + (fTemp31 - fTemp33) * self.fVec2[((i32::wrapping_sub(self.IOTA0, core::cmp::min(384001, core::cmp::max(0, i32::wrapping_add(1, iTemp32))))) & 524287) as usize];
		self.fRec1[0] = (if (F32::abs(fTemp34) > 1.1754944e-38) as i32 != 0 {fTemp34} else {0.0});
		*output0 = (self.fRec1[1]) as FaustFloat;
		let mut fTemp35: F32 = (fTemp4 * self.fRec11[1] - (self.fRec12[1] + self.fRec12[2])) / fTemp5;
		self.fRec11[0] = (if (F32::abs(-fTemp35) > 1.1754944e-38) as i32 != 0 {-fTemp35} else {0.0});
		let mut fTemp36: F32 = self.fRec3[0] * self.fRec11[0];
		self.fVec4[0] = fTemp36;
		let mut fTemp37: F32 = fTemp36 - self.fVec4[1];
		let mut fTemp38: F32 = 0.5 * fTemp36;
		let mut fTemp39: F32 = F32::abs(fTemp38);
		let mut fTemp40: F32 = Dsp_faustpower2_f(fTemp38);
		let mut fTemp41: F32 = 0.5 * self.fVec4[1];
		let mut fTemp42: F32 = F32::abs(fTemp41);
		let mut fTemp43: F32 = Dsp_faustpower2_f(fTemp41);
		let mut fTemp44: F32 = fTemp36 + self.fVec4[1];
		let mut fTemp45: F32 = 0.25 * fTemp44;
		let mut fTemp46: F32 = F32::abs(fTemp45);
		let mut fTemp47: F32 = (((fTemp45 > 0.0) as i32) - ((fTemp45 < 0.0) as i32)) as F32;
		let mut fTemp48: F32 = (*input1) as F32 + (if (F32::abs(0.5 * fTemp37) <= 0.001) as i32 != 0 {(if (fTemp46 < 0.33333334) as i32 != 0 {0.5 * fTemp44} else {(if (fTemp46 < 0.6666667) as i32 != 0 {0.33333334 * fTemp47 * (3.0 - Dsp_faustpower2_f(2.0 - 3.0 * fTemp46))} else {fTemp47})})} else {2.0 * (((if (fTemp39 < 0.33333334) as i32 != 0 {fTemp40} else {(if (fTemp39 < 0.6666667) as i32 != 0 {0.037037037 - (0.33333334 * fTemp39 + Dsp_faustpower3_f(fTemp39)) + 2.0 * fTemp40} else {-0.25925925 + fTemp39})}) - (if (fTemp42 < 0.33333334) as i32 != 0 {fTemp43} else {(if (fTemp42 < 0.6666667) as i32 != 0 {0.037037037 - (0.33333334 * fTemp42 + Dsp_faustpower3_f(fTemp42)) + 2.0 * fTemp43} else {-0.25925925 + fTemp42})})) / fTemp37)});
		self.fVec5[(self.IOTA0 & 524287) as usize] = fTemp48;
		let mut fTemp49: F32 = F32::max(0.0, -1.0 + self.fConst0 * (self.fRec4[0] + self.fRec5[0] * (fTemp30 + 0.0013 * self.ftbl1DspSIG1[iTemp25 as usize] + 1.77e-05 * self.ftbl1DspSIG1[iTemp28 as usize])));
		let mut iTemp50: i32 = (fTemp49) as i32;
		let mut fTemp51: F32 = F32::floor(fTemp49);
		let mut fTemp52: F32 = self.fVec5[((i32::wrapping_sub(self.IOTA0, core::cmp::min(384001, core::cmp::max(0, iTemp50)))) & 524287) as usize] * (1.0 - fTemp49 + fTemp51) + (fTemp49 - fTemp51) * self.fVec5[((i32::wrapping_sub(self.IOTA0, core::cmp::min(384001, core::cmp::max(0, i32::wrapping_add(1, iTemp50))))) & 524287) as usize];
		self.fRec12[0] = (if (F32::abs(fTemp52) > 1.1754944e-38) as i32 != 0 {fTemp52} else {0.0});
		*output1 = (self.fRec12[1]) as FaustFloat;
		self.iVec0[1] = self.iVec0[0];
		self.fRec2[1] = self.fRec2[0];
		self.fRec0[1] = self.fRec0[0];
		self.fRec3[1] = self.fRec3[0];
		self.fVec1[1] = self.fVec1[0];
		self.IOTA0 = i32::wrapping_add(self.IOTA0, 1);
		self.fRec4[1] = self.fRec4[0];
		self.fRec5[1] = self.fRec5[0];
		self.fRec7[1] = self.fRec7[0];
		self.fRec8[1] = self.fRec8[0];
		self.iRec10[1] = self.iRec10[0];
		self.fRec9[2] = self.fRec9[1];
		self.fRec9[1] = self.fRec9[0];
		self.fRec1[2] = self.fRec1[1];
		self.fRec1[1] = self.fRec1[0];
		self.fRec11[1] = self.fRec11[0];
		self.fVec4[1] = self.fVec4[0];
		self.fRec12[2] = self.fRec12[1];
		self.fRec12[1] = self.fRec12[0];
		
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

