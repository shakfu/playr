/* ------------------------------------------------------------
name: "zita"
Code generated with Faust 2.88.0 (https://faust.grame.fr)
Compilation options: -lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1
------------------------------------------------------------ */

#[repr(C)]
pub struct Dsp {
	fSampleRate: i32,
	fConst0: F32,
	fConst1: F32,
	fConst2: F32,
	fConst3: F32,
	fRec0: [F32;2],
	fRec1: [F32;2],
	fRec2: [F32;2],
	fRec3: [F32;2],
	fRec4: [F32;2],
	fRec5: [F32;2],
	fRec6: [F32;2],
	fConst4: F32,
	fConst5: F32,
	fHslider0: FaustFloat,
	fSlow0: F32,
	fSlow1: F32,
	fSlow2: F32,
	fConst6: F32,
	fHslider1: FaustFloat,
	fSlow3: F32,
	fSlow4: F32,
	fSlow5: F32,
	fSlow6: F32,
	fSlow7: F32,
	fSlow8: F32,
	fSlow9: F32,
	fRec7: [F32;2],
	fConst7: F32,
	fConst8: F32,
	fSlow10: F32,
	fSlow11: F32,
	fSlow12: F32,
	fSlow13: F32,
	fSlow14: F32,
	fSlow15: F32,
	fSlow16: F32,
	fSlow17: F32,
	fRec8: [F32;2],
	fRec9: [F32;2],
	IOTA0: i32,
	fVec0: [F32;32768],
	fConst9: F32,
	iConst10: i32,
	fVec1: [F32;8192],
	fConst11: F32,
	fConst12: F32,
	fSlow18: F32,
	fSlow19: F32,
	fSlow20: F32,
	fSlow21: F32,
	fSlow22: F32,
	fSlow23: F32,
	fSlow24: F32,
	fSlow25: F32,
	fRec11: [F32;2],
	fVec2: [F32;65536],
	fConst13: F32,
	iConst14: i32,
	fVec3: [F32;8192],
	iConst15: i32,
	fRec12: [F32;2],
	fConst16: F32,
	fConst17: F32,
	fSlow26: F32,
	fSlow27: F32,
	fSlow28: F32,
	fSlow29: F32,
	fSlow30: F32,
	fSlow31: F32,
	fSlow32: F32,
	fSlow33: F32,
	fRec13: [F32;2],
	fVec4: [F32;65536],
	fConst18: F32,
	iConst19: i32,
	fVec5: [F32;8192],
	fVec6: [F32;32768],
	fConst20: F32,
	iConst21: i32,
	fVec7: [F32;4096],
	iConst22: i32,
	fRec15: [F32;2],
	fConst23: F32,
	fConst24: F32,
	fSlow34: F32,
	fSlow35: F32,
	fSlow36: F32,
	fSlow37: F32,
	fSlow38: F32,
	fSlow39: F32,
	fSlow40: F32,
	fSlow41: F32,
	fRec16: [F32;2],
	fConst25: F32,
	fConst26: F32,
	fSlow42: F32,
	fSlow43: F32,
	fSlow44: F32,
	fSlow45: F32,
	fSlow46: F32,
	fSlow47: F32,
	fSlow48: F32,
	fSlow49: F32,
	fRec17: [F32;2],
	fConst27: F32,
	fConst28: F32,
	fSlow50: F32,
	fSlow51: F32,
	fSlow52: F32,
	fSlow53: F32,
	fSlow54: F32,
	fSlow55: F32,
	fSlow56: F32,
	fSlow57: F32,
	fRec18: [F32;2],
	fConst29: F32,
	fConst30: F32,
	fSlow58: F32,
	fSlow59: F32,
	fSlow60: F32,
	fSlow61: F32,
	fSlow62: F32,
	fSlow63: F32,
	fSlow64: F32,
	fSlow65: F32,
	fRec19: [F32;2],
	iConst31: i32,
	fRec21: [F32;2],
	fVec8: [F32;65536],
	fConst32: F32,
	iConst33: i32,
	fVec9: [F32;8192],
	iConst34: i32,
	fRec23: [F32;2],
	fVec10: [F32;65536],
	fConst35: F32,
	iConst36: i32,
	fVec11: [F32;8192],
	iConst37: i32,
	fRec24: [F32;2],
	fVec12: [F32;32768],
	fConst38: F32,
	iConst39: i32,
	fVec13: [F32;4096],
	fVec14: [F32;65536],
	fConst40: F32,
	iConst41: i32,
	iConst42: i32,
	fRec28: [F32;2],
	iConst43: i32,
	fRec30: [F32;2],
	fVec15: [F32;4096],
	iConst44: i32,
	fRec31: [F32;2],
	fRec32: [F32;3],
	fRec33: [F32;3],
	fRec34: [F32;3],
	fRec35: [F32;3],
	fRec36: [F32;3],
	fRec37: [F32;3],
	fRec38: [F32;3],
	fRec39: [F32;3],
}


fn Dsp_faustpower2_f(value: F32) -> F32 {
	return value * value;
}
pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 2;
pub const FAUST_ACTIVES: usize = 2;
pub const FAUST_PASSIVES: usize = 0;

impl Dsp {
		
	pub fn new() -> Dsp { 
		Dsp {
			fSampleRate: 0,
			fConst0: 0.0,
			fConst1: 0.0,
			fConst2: 0.0,
			fConst3: 0.0,
			fRec0: [0.0;2],
			fRec1: [0.0;2],
			fRec2: [0.0;2],
			fRec3: [0.0;2],
			fRec4: [0.0;2],
			fRec5: [0.0;2],
			fRec6: [0.0;2],
			fConst4: 0.0,
			fConst5: 0.0,
			fHslider0: 0.0,
			fSlow0: 0.0,
			fSlow1: 0.0,
			fSlow2: 0.0,
			fConst6: 0.0,
			fHslider1: 0.0,
			fSlow3: 0.0,
			fSlow4: 0.0,
			fSlow5: 0.0,
			fSlow6: 0.0,
			fSlow7: 0.0,
			fSlow8: 0.0,
			fSlow9: 0.0,
			fRec7: [0.0;2],
			fConst7: 0.0,
			fConst8: 0.0,
			fSlow10: 0.0,
			fSlow11: 0.0,
			fSlow12: 0.0,
			fSlow13: 0.0,
			fSlow14: 0.0,
			fSlow15: 0.0,
			fSlow16: 0.0,
			fSlow17: 0.0,
			fRec8: [0.0;2],
			fRec9: [0.0;2],
			IOTA0: 0,
			fVec0: [0.0;32768],
			fConst9: 0.0,
			iConst10: 0,
			fVec1: [0.0;8192],
			fConst11: 0.0,
			fConst12: 0.0,
			fSlow18: 0.0,
			fSlow19: 0.0,
			fSlow20: 0.0,
			fSlow21: 0.0,
			fSlow22: 0.0,
			fSlow23: 0.0,
			fSlow24: 0.0,
			fSlow25: 0.0,
			fRec11: [0.0;2],
			fVec2: [0.0;65536],
			fConst13: 0.0,
			iConst14: 0,
			fVec3: [0.0;8192],
			iConst15: 0,
			fRec12: [0.0;2],
			fConst16: 0.0,
			fConst17: 0.0,
			fSlow26: 0.0,
			fSlow27: 0.0,
			fSlow28: 0.0,
			fSlow29: 0.0,
			fSlow30: 0.0,
			fSlow31: 0.0,
			fSlow32: 0.0,
			fSlow33: 0.0,
			fRec13: [0.0;2],
			fVec4: [0.0;65536],
			fConst18: 0.0,
			iConst19: 0,
			fVec5: [0.0;8192],
			fVec6: [0.0;32768],
			fConst20: 0.0,
			iConst21: 0,
			fVec7: [0.0;4096],
			iConst22: 0,
			fRec15: [0.0;2],
			fConst23: 0.0,
			fConst24: 0.0,
			fSlow34: 0.0,
			fSlow35: 0.0,
			fSlow36: 0.0,
			fSlow37: 0.0,
			fSlow38: 0.0,
			fSlow39: 0.0,
			fSlow40: 0.0,
			fSlow41: 0.0,
			fRec16: [0.0;2],
			fConst25: 0.0,
			fConst26: 0.0,
			fSlow42: 0.0,
			fSlow43: 0.0,
			fSlow44: 0.0,
			fSlow45: 0.0,
			fSlow46: 0.0,
			fSlow47: 0.0,
			fSlow48: 0.0,
			fSlow49: 0.0,
			fRec17: [0.0;2],
			fConst27: 0.0,
			fConst28: 0.0,
			fSlow50: 0.0,
			fSlow51: 0.0,
			fSlow52: 0.0,
			fSlow53: 0.0,
			fSlow54: 0.0,
			fSlow55: 0.0,
			fSlow56: 0.0,
			fSlow57: 0.0,
			fRec18: [0.0;2],
			fConst29: 0.0,
			fConst30: 0.0,
			fSlow58: 0.0,
			fSlow59: 0.0,
			fSlow60: 0.0,
			fSlow61: 0.0,
			fSlow62: 0.0,
			fSlow63: 0.0,
			fSlow64: 0.0,
			fSlow65: 0.0,
			fRec19: [0.0;2],
			iConst31: 0,
			fRec21: [0.0;2],
			fVec8: [0.0;65536],
			fConst32: 0.0,
			iConst33: 0,
			fVec9: [0.0;8192],
			iConst34: 0,
			fRec23: [0.0;2],
			fVec10: [0.0;65536],
			fConst35: 0.0,
			iConst36: 0,
			fVec11: [0.0;8192],
			iConst37: 0,
			fRec24: [0.0;2],
			fVec12: [0.0;32768],
			fConst38: 0.0,
			iConst39: 0,
			fVec13: [0.0;4096],
			fVec14: [0.0;65536],
			fConst40: 0.0,
			iConst41: 0,
			iConst42: 0,
			fRec28: [0.0;2],
			iConst43: 0,
			fRec30: [0.0;2],
			fVec15: [0.0;4096],
			iConst44: 0,
			fRec31: [0.0;2],
			fRec32: [0.0;3],
			fRec33: [0.0;3],
			fRec34: [0.0;3],
			fRec35: [0.0;3],
			fRec36: [0.0;3],
			fRec37: [0.0;3],
			fRec38: [0.0;3],
			fRec39: [0.0;3],
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("basics.lib/name", r"Faust Basic Element Library");
		m.declare("basics.lib/version", r"1.23.0");
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1");
		m.declare("delays.lib/name", r"Faust Delay Library");
		m.declare("delays.lib/version", r"1.2.0");
		m.declare("filename", r"zita.dsp");
		m.declare("filters.lib/allpass_comb:author", r"Julius O. Smith III");
		m.declare("filters.lib/allpass_comb:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/allpass_comb:license", r"LicenseRef-STK-4.3");
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
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"zita");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("reverbs.lib/name", r"Faust Reverb Library");
		m.declare("reverbs.lib/version", r"1.5.1");
		m.declare("routes.lib/hadamard:author", r"Remy Muller, revised by Romain Michon");
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
		self.fHslider0 = (2.0) as FaustFloat;
		self.fHslider1 = (0.3) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		for l0 in 0..2 {
			self.fRec0[l0 as usize] = 0.0;
		}
		for l1 in 0..2 {
			self.fRec1[l1 as usize] = 0.0;
		}
		for l2 in 0..2 {
			self.fRec2[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fRec3[l3 as usize] = 0.0;
		}
		for l4 in 0..2 {
			self.fRec4[l4 as usize] = 0.0;
		}
		for l5 in 0..2 {
			self.fRec5[l5 as usize] = 0.0;
		}
		for l6 in 0..2 {
			self.fRec6[l6 as usize] = 0.0;
		}
		for l7 in 0..2 {
			self.fRec7[l7 as usize] = 0.0;
		}
		for l8 in 0..2 {
			self.fRec8[l8 as usize] = 0.0;
		}
		for l9 in 0..2 {
			self.fRec9[l9 as usize] = 0.0;
		}
		self.IOTA0 = 0;
		for l10 in 0..32768 {
			self.fVec0[l10 as usize] = 0.0;
		}
		for l11 in 0..8192 {
			self.fVec1[l11 as usize] = 0.0;
		}
		for l12 in 0..2 {
			self.fRec11[l12 as usize] = 0.0;
		}
		for l13 in 0..65536 {
			self.fVec2[l13 as usize] = 0.0;
		}
		for l14 in 0..8192 {
			self.fVec3[l14 as usize] = 0.0;
		}
		for l15 in 0..2 {
			self.fRec12[l15 as usize] = 0.0;
		}
		for l16 in 0..2 {
			self.fRec13[l16 as usize] = 0.0;
		}
		for l17 in 0..65536 {
			self.fVec4[l17 as usize] = 0.0;
		}
		for l18 in 0..8192 {
			self.fVec5[l18 as usize] = 0.0;
		}
		for l19 in 0..32768 {
			self.fVec6[l19 as usize] = 0.0;
		}
		for l20 in 0..4096 {
			self.fVec7[l20 as usize] = 0.0;
		}
		for l21 in 0..2 {
			self.fRec15[l21 as usize] = 0.0;
		}
		for l22 in 0..2 {
			self.fRec16[l22 as usize] = 0.0;
		}
		for l23 in 0..2 {
			self.fRec17[l23 as usize] = 0.0;
		}
		for l24 in 0..2 {
			self.fRec18[l24 as usize] = 0.0;
		}
		for l25 in 0..2 {
			self.fRec19[l25 as usize] = 0.0;
		}
		for l26 in 0..2 {
			self.fRec21[l26 as usize] = 0.0;
		}
		for l27 in 0..65536 {
			self.fVec8[l27 as usize] = 0.0;
		}
		for l28 in 0..8192 {
			self.fVec9[l28 as usize] = 0.0;
		}
		for l29 in 0..2 {
			self.fRec23[l29 as usize] = 0.0;
		}
		for l30 in 0..65536 {
			self.fVec10[l30 as usize] = 0.0;
		}
		for l31 in 0..8192 {
			self.fVec11[l31 as usize] = 0.0;
		}
		for l32 in 0..2 {
			self.fRec24[l32 as usize] = 0.0;
		}
		for l33 in 0..32768 {
			self.fVec12[l33 as usize] = 0.0;
		}
		for l34 in 0..4096 {
			self.fVec13[l34 as usize] = 0.0;
		}
		for l35 in 0..65536 {
			self.fVec14[l35 as usize] = 0.0;
		}
		for l36 in 0..2 {
			self.fRec28[l36 as usize] = 0.0;
		}
		for l37 in 0..2 {
			self.fRec30[l37 as usize] = 0.0;
		}
		for l38 in 0..4096 {
			self.fVec15[l38 as usize] = 0.0;
		}
		for l39 in 0..2 {
			self.fRec31[l39 as usize] = 0.0;
		}
		for l40 in 0..3 {
			self.fRec32[l40 as usize] = 0.0;
		}
		for l41 in 0..3 {
			self.fRec33[l41 as usize] = 0.0;
		}
		for l42 in 0..3 {
			self.fRec34[l42 as usize] = 0.0;
		}
		for l43 in 0..3 {
			self.fRec35[l43 as usize] = 0.0;
		}
		for l44 in 0..3 {
			self.fRec36[l44 as usize] = 0.0;
		}
		for l45 in 0..3 {
			self.fRec37[l45 as usize] = 0.0;
		}
		for l46 in 0..3 {
			self.fRec38[l46 as usize] = 0.0;
		}
		for l47 in 0..3 {
			self.fRec39[l47 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
		self.fConst1 = 1.0 / F32::tan(628.31854 / self.fConst0);
		self.fConst2 = 1.0 / (self.fConst1 + 1.0);
		self.fConst3 = 1.0 - self.fConst1;
		self.fConst4 = F32::floor(0.127837 * self.fConst0 + 0.5);
		self.fConst5 = 6.9077554 * (self.fConst4 / self.fConst0);
		self.fConst6 = 125663.7 / self.fConst0;
		self.fConst7 = F32::floor(0.153129 * self.fConst0 + 0.5);
		self.fConst8 = 6.9077554 * (self.fConst7 / self.fConst0);
		self.fConst9 = F32::floor(0.031604 * self.fConst0 + 0.5);
		self.iConst10 = (F32::min(32768.0, F32::max(0.0, self.fConst4 - self.fConst9))) as i32;
		self.fConst11 = F32::floor(0.256891 * self.fConst0 + 0.5);
		self.fConst12 = 6.9077554 * (self.fConst11 / self.fConst0);
		self.fConst13 = F32::floor(0.027333 * self.fConst0 + 0.5);
		self.iConst14 = (F32::min(65536.0, F32::max(0.0, self.fConst11 - self.fConst13))) as i32;
		self.iConst15 = (F32::min(8192.0, F32::max(0.0, self.fConst13 + -1.0))) as i32;
		self.fConst16 = F32::floor(0.174713 * self.fConst0 + 0.5);
		self.fConst17 = 6.9077554 * (self.fConst16 / self.fConst0);
		self.fConst18 = F32::floor(0.022904 * self.fConst0 + 0.5);
		self.iConst19 = (F32::min(32768.0, F32::max(0.0, self.fConst16 - self.fConst18))) as i32;
		self.fConst20 = F32::floor(0.020346 * self.fConst0 + 0.5);
		self.iConst21 = (F32::min(32768.0, F32::max(0.0, self.fConst7 - self.fConst20))) as i32;
		self.iConst22 = (F32::min(4096.0, F32::max(0.0, self.fConst20 + -1.0))) as i32;
		self.fConst23 = F32::floor(0.125 * self.fConst0 + 0.5);
		self.fConst24 = 6.9077554 * (self.fConst23 / self.fConst0);
		self.fConst25 = F32::floor(0.192303 * self.fConst0 + 0.5);
		self.fConst26 = 6.9077554 * (self.fConst25 / self.fConst0);
		self.fConst27 = F32::floor(0.210389 * self.fConst0 + 0.5);
		self.fConst28 = 6.9077554 * (self.fConst27 / self.fConst0);
		self.fConst29 = F32::floor(0.219991 * self.fConst0 + 0.5);
		self.fConst30 = 6.9077554 * (self.fConst29 / self.fConst0);
		self.iConst31 = (F32::min(8192.0, F32::max(0.0, self.fConst9 + -1.0))) as i32;
		self.fConst32 = F32::floor(0.024421 * self.fConst0 + 0.5);
		self.iConst33 = (F32::min(65536.0, F32::max(0.0, self.fConst27 - self.fConst32))) as i32;
		self.iConst34 = (F32::min(8192.0, F32::max(0.0, self.fConst18 + -1.0))) as i32;
		self.fConst35 = F32::floor(0.029291 * self.fConst0 + 0.5);
		self.iConst36 = (F32::min(32768.0, F32::max(0.0, self.fConst25 - self.fConst35))) as i32;
		self.iConst37 = (F32::min(8192.0, F32::max(0.0, self.fConst35 + -1.0))) as i32;
		self.fConst38 = F32::floor(0.013458 * self.fConst0 + 0.5);
		self.iConst39 = (F32::min(32768.0, F32::max(0.0, self.fConst23 - self.fConst38))) as i32;
		self.fConst40 = F32::floor(0.019123 * self.fConst0 + 0.5);
		self.iConst41 = (F32::min(65536.0, F32::max(0.0, self.fConst29 - self.fConst40))) as i32;
		self.iConst42 = (F32::min(4096.0, F32::max(0.0, self.fConst38 + -1.0))) as i32;
		self.iConst43 = (F32::min(8192.0, F32::max(0.0, self.fConst32 + -1.0))) as i32;
		self.iConst44 = (F32::min(4096.0, F32::max(0.0, self.fConst40 + -1.0))) as i32;
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
		ui_interface.open_vertical_box("zita");
		ui_interface.add_horizontal_slider("damp", ParamIndex(0), 0.3, 0.0, 1.0, 0.001);
		ui_interface.add_horizontal_slider("decay", ParamIndex(1), 2.0, 0.1, 2e+01, 0.01);
		ui_interface.close_box();
	}
	
	pub fn get_param(&self, param: ParamIndex) -> Option<FaustFloat> {
		match param.0 {
			1 => Some(self.fHslider0),
			0 => Some(self.fHslider1),
			_ => None,
		}
	}
	
	pub fn set_param(&mut self, param: ParamIndex, value: FaustFloat) {
		match param.0 {
			1 => { self.fHslider0 = value }
			0 => { self.fHslider1 = value }
			_ => {}
		}
	}
	
	pub fn control(&mut self) {
		// Obtaining locks on 0 static var(s)
	self.fSlow0 = (self.fHslider0) as F32;
		self.fSlow1 = F32::exp(-(self.fConst5 / self.fSlow0));
		self.fSlow2 = Dsp_faustpower2_f(self.fSlow1);
		self.fSlow3 = F32::cos(self.fConst6 * F32::powf(0.075, (self.fHslider1) as F32));
		self.fSlow4 = 1.0 - self.fSlow2 * self.fSlow3;
		self.fSlow5 = 1.0 - self.fSlow2;
		self.fSlow6 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow4) / Dsp_faustpower2_f(self.fSlow5) + -1.0));
		self.fSlow7 = self.fSlow4 / self.fSlow5;
		self.fSlow8 = self.fSlow1 * (self.fSlow6 + (1.0 - self.fSlow7));
		self.fSlow9 = self.fSlow7 - self.fSlow6;
		self.fSlow10 = F32::exp(-(self.fConst8 / self.fSlow0));
		self.fSlow11 = Dsp_faustpower2_f(self.fSlow10);
		self.fSlow12 = 1.0 - self.fSlow3 * self.fSlow11;
		self.fSlow13 = 1.0 - self.fSlow11;
		self.fSlow14 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow12) / Dsp_faustpower2_f(self.fSlow13) + -1.0));
		self.fSlow15 = self.fSlow12 / self.fSlow13;
		self.fSlow16 = self.fSlow10 * (self.fSlow14 + (1.0 - self.fSlow15));
		self.fSlow17 = self.fSlow15 - self.fSlow14;
		self.fSlow18 = F32::exp(-(self.fConst12 / self.fSlow0));
		self.fSlow19 = Dsp_faustpower2_f(self.fSlow18);
		self.fSlow20 = 1.0 - self.fSlow3 * self.fSlow19;
		self.fSlow21 = 1.0 - self.fSlow19;
		self.fSlow22 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow20) / Dsp_faustpower2_f(self.fSlow21) + -1.0));
		self.fSlow23 = self.fSlow20 / self.fSlow21;
		self.fSlow24 = self.fSlow18 * (self.fSlow22 + (1.0 - self.fSlow23));
		self.fSlow25 = self.fSlow23 - self.fSlow22;
		self.fSlow26 = F32::exp(-(self.fConst17 / self.fSlow0));
		self.fSlow27 = Dsp_faustpower2_f(self.fSlow26);
		self.fSlow28 = 1.0 - self.fSlow3 * self.fSlow27;
		self.fSlow29 = 1.0 - self.fSlow27;
		self.fSlow30 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow28) / Dsp_faustpower2_f(self.fSlow29) + -1.0));
		self.fSlow31 = self.fSlow28 / self.fSlow29;
		self.fSlow32 = self.fSlow26 * (self.fSlow30 + (1.0 - self.fSlow31));
		self.fSlow33 = self.fSlow31 - self.fSlow30;
		self.fSlow34 = F32::exp(-(self.fConst24 / self.fSlow0));
		self.fSlow35 = Dsp_faustpower2_f(self.fSlow34);
		self.fSlow36 = 1.0 - self.fSlow3 * self.fSlow35;
		self.fSlow37 = 1.0 - self.fSlow35;
		self.fSlow38 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow36) / Dsp_faustpower2_f(self.fSlow37) + -1.0));
		self.fSlow39 = self.fSlow36 / self.fSlow37;
		self.fSlow40 = self.fSlow34 * (self.fSlow38 + (1.0 - self.fSlow39));
		self.fSlow41 = self.fSlow39 - self.fSlow38;
		self.fSlow42 = F32::exp(-(self.fConst26 / self.fSlow0));
		self.fSlow43 = Dsp_faustpower2_f(self.fSlow42);
		self.fSlow44 = 1.0 - self.fSlow3 * self.fSlow43;
		self.fSlow45 = 1.0 - self.fSlow43;
		self.fSlow46 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow44) / Dsp_faustpower2_f(self.fSlow45) + -1.0));
		self.fSlow47 = self.fSlow44 / self.fSlow45;
		self.fSlow48 = self.fSlow42 * (self.fSlow46 + (1.0 - self.fSlow47));
		self.fSlow49 = self.fSlow47 - self.fSlow46;
		self.fSlow50 = F32::exp(-(self.fConst28 / self.fSlow0));
		self.fSlow51 = Dsp_faustpower2_f(self.fSlow50);
		self.fSlow52 = 1.0 - self.fSlow3 * self.fSlow51;
		self.fSlow53 = 1.0 - self.fSlow51;
		self.fSlow54 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow52) / Dsp_faustpower2_f(self.fSlow53) + -1.0));
		self.fSlow55 = self.fSlow52 / self.fSlow53;
		self.fSlow56 = self.fSlow50 * (self.fSlow54 + (1.0 - self.fSlow55));
		self.fSlow57 = self.fSlow55 - self.fSlow54;
		self.fSlow58 = F32::exp(-(self.fConst30 / self.fSlow0));
		self.fSlow59 = Dsp_faustpower2_f(self.fSlow58);
		self.fSlow60 = 1.0 - self.fSlow3 * self.fSlow59;
		self.fSlow61 = 1.0 - self.fSlow59;
		self.fSlow62 = F32::sqrt(F32::max(0.0, Dsp_faustpower2_f(self.fSlow60) / Dsp_faustpower2_f(self.fSlow61) + -1.0));
		self.fSlow63 = self.fSlow60 / self.fSlow61;
		self.fSlow64 = self.fSlow58 * (self.fSlow62 + (1.0 - self.fSlow63));
		self.fSlow65 = self.fSlow63 - self.fSlow62;
	}
	
	pub fn frame(&mut self, inputs: &[&FaustFloat], outputs: &mut [&mut FaustFloat]) {
		let input0 = inputs[0];
		let input1 = inputs[1];
		let (nextoutput, outputs): (&mut [&mut f64], &mut [&mut f64]) = outputs.split_at_mut(1);
		let output0: &mut FaustFloat = nextoutput[0];
		let (nextoutput, outputs): (&mut [&mut f64], &mut [&mut f64]) = outputs.split_at_mut(1);
		let output1: &mut FaustFloat = nextoutput[0];// Obtaining locks on 0 static var(s)
		
		
		// generateOneSample
		let mut fTemp0: F32 = self.fConst2 * (self.fConst3 * self.fRec0[1] - (self.fRec33[1] + self.fRec33[2]));
		self.fRec0[0] = (if (F32::abs(-fTemp0) > 1.1754944e-38) as i32 != 0 {-fTemp0} else {0.0});
		let mut fTemp1: F32 = self.fConst2 * (self.fConst3 * self.fRec1[1] - (self.fRec34[1] + self.fRec34[2]));
		self.fRec1[0] = (if (F32::abs(-fTemp1) > 1.1754944e-38) as i32 != 0 {-fTemp1} else {0.0});
		let mut fTemp2: F32 = self.fConst2 * (self.fConst3 * self.fRec2[1] - (self.fRec38[1] + self.fRec38[2]));
		self.fRec2[0] = (if (F32::abs(-fTemp2) > 1.1754944e-38) as i32 != 0 {-fTemp2} else {0.0});
		let mut fTemp3: F32 = self.fConst2 * (self.fConst3 * self.fRec3[1] - (self.fRec37[1] + self.fRec37[2]));
		self.fRec3[0] = (if (F32::abs(-fTemp3) > 1.1754944e-38) as i32 != 0 {-fTemp3} else {0.0});
		let mut fTemp4: F32 = self.fConst2 * (self.fConst3 * self.fRec4[1] - (self.fRec32[1] + self.fRec32[2]));
		self.fRec4[0] = (if (F32::abs(-fTemp4) > 1.1754944e-38) as i32 != 0 {-fTemp4} else {0.0});
		let mut fTemp5: F32 = self.fConst2 * (self.fConst3 * self.fRec5[1] - (self.fRec35[1] + self.fRec35[2]));
		self.fRec5[0] = (if (F32::abs(-fTemp5) > 1.1754944e-38) as i32 != 0 {-fTemp5} else {0.0});
		let mut fTemp6: F32 = self.fConst2 * (self.fConst3 * self.fRec6[1] - (self.fRec39[1] + self.fRec39[2]));
		self.fRec6[0] = (if (F32::abs(-fTemp6) > 1.1754944e-38) as i32 != 0 {-fTemp6} else {0.0});
		let mut fTemp7: F32 = self.fSlow8 * self.fRec33[1] + self.fSlow9 * self.fRec7[1];
		self.fRec7[0] = (if (F32::abs(fTemp7) > 1.1754944e-38) as i32 != 0 {fTemp7} else {0.0});
		let mut fTemp8: F32 = self.fSlow16 * self.fRec34[1] + self.fSlow17 * self.fRec8[1];
		self.fRec8[0] = (if (F32::abs(fTemp8) > 1.1754944e-38) as i32 != 0 {fTemp8} else {0.0});
		let mut fTemp9: F32 = self.fConst2 * (self.fConst3 * self.fRec9[1] - (self.fRec36[1] + self.fRec36[2]));
		self.fRec9[0] = (if (F32::abs(-fTemp9) > 1.1754944e-38) as i32 != 0 {-fTemp9} else {0.0});
		self.fVec0[(self.IOTA0 & 32767) as usize] = 1e-20 + 0.35355338 * self.fRec7[0];
		let mut fTemp10: F32 = 0.3 * (*input0) as F32;
		let mut fTemp11: F32 = self.fVec0[((i32::wrapping_sub(self.IOTA0, self.iConst10)) & 32767) as usize] - (fTemp10 + 0.6 * self.fRec21[1]);
		self.fVec1[(self.IOTA0 & 8191) as usize] = fTemp11;
		let mut fTemp12: F32 = 0.6 * fTemp11;
		let mut fRec10: F32 = (if (F32::abs(fTemp12) > 1.1754944e-38) as i32 != 0 {fTemp12} else {0.0});
		let mut fTemp13: F32 = self.fSlow24 * self.fRec32[1] + self.fSlow25 * self.fRec11[1];
		self.fRec11[0] = (if (F32::abs(fTemp13) > 1.1754944e-38) as i32 != 0 {fTemp13} else {0.0});
		self.fVec2[(self.IOTA0 & 65535) as usize] = 1e-20 + 0.35355338 * self.fRec11[0];
		let mut fTemp14: F32 = 0.6 * self.fRec12[1] + self.fVec2[((i32::wrapping_sub(self.IOTA0, self.iConst14)) & 65535) as usize];
		let mut fTemp15: F32 = 0.3 * (*input1) as F32;
		self.fVec3[(self.IOTA0 & 8191) as usize] = fTemp14 - fTemp15;
		let mut fTemp16: F32 = self.fVec3[((i32::wrapping_sub(self.IOTA0, self.iConst15)) & 8191) as usize];
		self.fRec12[0] = (if (F32::abs(fTemp16) > 1.1754944e-38) as i32 != 0 {fTemp16} else {0.0});
		let mut fTemp17: F32 = self.fSlow32 * self.fRec38[1] + self.fSlow33 * self.fRec13[1];
		self.fRec13[0] = (if (F32::abs(fTemp17) > 1.1754944e-38) as i32 != 0 {fTemp17} else {0.0});
		self.fVec4[(self.IOTA0 & 65535) as usize] = 1e-20 + 0.35355338 * self.fRec13[0];
		let mut fTemp18: F32 = fTemp10 - 0.6 * self.fRec23[1] + self.fVec4[((i32::wrapping_sub(self.IOTA0, self.iConst19)) & 65535) as usize];
		self.fVec5[(self.IOTA0 & 8191) as usize] = fTemp18;
		let mut fTemp19: F32 = 0.6 * fTemp18;
		let mut fRec14: F32 = (if (F32::abs(fTemp19) > 1.1754944e-38) as i32 != 0 {fTemp19} else {0.0});
		self.fVec6[(self.IOTA0 & 32767) as usize] = 1e-20 + 0.35355338 * self.fRec8[0];
		let mut fTemp20: F32 = fTemp10 - 0.6 * self.fRec15[1] + self.fVec6[((i32::wrapping_sub(self.IOTA0, self.iConst21)) & 32767) as usize];
		self.fVec7[(self.IOTA0 & 4095) as usize] = fTemp20;
		let mut fTemp21: F32 = self.fVec7[((i32::wrapping_sub(self.IOTA0, self.iConst22)) & 4095) as usize];
		self.fRec15[0] = (if (F32::abs(fTemp21) > 1.1754944e-38) as i32 != 0 {fTemp21} else {0.0});
		let mut fTemp22: F32 = self.fSlow40 * self.fRec36[1] + self.fSlow41 * self.fRec16[1];
		self.fRec16[0] = (if (F32::abs(fTemp22) > 1.1754944e-38) as i32 != 0 {fTemp22} else {0.0});
		let mut fTemp23: F32 = self.fSlow48 * self.fRec39[1] + self.fSlow49 * self.fRec17[1];
		self.fRec17[0] = (if (F32::abs(fTemp23) > 1.1754944e-38) as i32 != 0 {fTemp23} else {0.0});
		let mut fTemp24: F32 = self.fSlow56 * self.fRec35[1] + self.fSlow57 * self.fRec18[1];
		self.fRec18[0] = (if (F32::abs(fTemp24) > 1.1754944e-38) as i32 != 0 {fTemp24} else {0.0});
		let mut fTemp25: F32 = self.fSlow64 * self.fRec37[1] + self.fSlow65 * self.fRec19[1];
		self.fRec19[0] = (if (F32::abs(fTemp25) > 1.1754944e-38) as i32 != 0 {fTemp25} else {0.0});
		let mut fTemp26: F32 = 0.6 * fTemp20;
		let mut fRec20: F32 = (if (F32::abs(fTemp26) > 1.1754944e-38) as i32 != 0 {fTemp26} else {0.0});
		let mut fTemp27: F32 = self.fVec1[((i32::wrapping_sub(self.IOTA0, self.iConst31)) & 8191) as usize];
		self.fRec21[0] = (if (F32::abs(fTemp27) > 1.1754944e-38) as i32 != 0 {fTemp27} else {0.0});
		self.fVec8[(self.IOTA0 & 65535) as usize] = 1e-20 + 0.35355338 * self.fRec18[0];
		let mut fTemp28: F32 = fTemp15 + 0.6 * self.fRec30[1] + self.fVec8[((i32::wrapping_sub(self.IOTA0, self.iConst33)) & 65535) as usize];
		self.fVec9[(self.IOTA0 & 8191) as usize] = fTemp28;
		let mut fTemp29: F32 = 0.6 * fTemp28;
		let mut fRec22: F32 = (if (F32::abs(-fTemp29) > 1.1754944e-38) as i32 != 0 {-fTemp29} else {0.0});
		let mut fTemp30: F32 = self.fVec5[((i32::wrapping_sub(self.IOTA0, self.iConst34)) & 8191) as usize];
		self.fRec23[0] = (if (F32::abs(fTemp30) > 1.1754944e-38) as i32 != 0 {fTemp30} else {0.0});
		self.fVec10[(self.IOTA0 & 65535) as usize] = 1e-20 + 0.35355338 * self.fRec17[0];
		let mut fTemp31: F32 = fTemp15 + 0.6 * self.fRec24[1] + self.fVec10[((i32::wrapping_sub(self.IOTA0, self.iConst36)) & 65535) as usize];
		self.fVec11[(self.IOTA0 & 8191) as usize] = fTemp31;
		let mut fTemp32: F32 = self.fVec11[((i32::wrapping_sub(self.IOTA0, self.iConst37)) & 8191) as usize];
		self.fRec24[0] = (if (F32::abs(fTemp32) > 1.1754944e-38) as i32 != 0 {fTemp32} else {0.0});
		self.fVec12[(self.IOTA0 & 32767) as usize] = 1e-20 + 0.35355338 * self.fRec16[0];
		let mut fTemp33: F32 = self.fVec12[((i32::wrapping_sub(self.IOTA0, self.iConst39)) & 32767) as usize] - (fTemp10 + 0.6 * self.fRec28[1]);
		self.fVec13[(self.IOTA0 & 4095) as usize] = fTemp33;
		let mut fTemp34: F32 = 0.6 * fTemp33;
		let mut fRec25: F32 = (if (F32::abs(fTemp34) > 1.1754944e-38) as i32 != 0 {fTemp34} else {0.0});
		let mut fTemp35: F32 = 0.6 * (fTemp15 - fTemp14);
		let mut fRec26: F32 = (if (F32::abs(fTemp35) > 1.1754944e-38) as i32 != 0 {fTemp35} else {0.0});
		self.fVec14[(self.IOTA0 & 65535) as usize] = 1e-20 + 0.35355338 * self.fRec19[0];
		let mut fTemp36: F32 = 0.6 * self.fRec31[1] + self.fVec14[((i32::wrapping_sub(self.IOTA0, self.iConst41)) & 65535) as usize];
		let mut fTemp37: F32 = 0.6 * (fTemp15 - fTemp36);
		let mut fRec27: F32 = (if (F32::abs(fTemp37) > 1.1754944e-38) as i32 != 0 {fTemp37} else {0.0});
		let mut fTemp38: F32 = self.fVec13[((i32::wrapping_sub(self.IOTA0, self.iConst42)) & 4095) as usize];
		self.fRec28[0] = (if (F32::abs(fTemp38) > 1.1754944e-38) as i32 != 0 {fTemp38} else {0.0});
		let mut fTemp39: F32 = 0.6 * fTemp31;
		let mut fRec29: F32 = (if (F32::abs(-fTemp39) > 1.1754944e-38) as i32 != 0 {-fTemp39} else {0.0});
		let mut fTemp40: F32 = self.fVec9[((i32::wrapping_sub(self.IOTA0, self.iConst43)) & 8191) as usize];
		self.fRec30[0] = (if (F32::abs(fTemp40) > 1.1754944e-38) as i32 != 0 {fTemp40} else {0.0});
		self.fVec15[(self.IOTA0 & 4095) as usize] = fTemp36 - fTemp15;
		let mut fTemp41: F32 = self.fVec15[((i32::wrapping_sub(self.IOTA0, self.iConst44)) & 4095) as usize];
		self.fRec31[0] = (if (F32::abs(fTemp41) > 1.1754944e-38) as i32 != 0 {fTemp41} else {0.0});
		let mut fTemp42: F32 = self.fRec12[1] + self.fRec23[1];
		let mut fTemp43: F32 = fRec27 + fRec26 + fRec14 + fRec20 + self.fRec31[1] + self.fRec15[1] + fTemp42 - (fRec29 + fRec22 + fRec25 + fRec10 + self.fRec28[1] + self.fRec24[1] + self.fRec21[1] + self.fRec30[1]);
		self.fRec32[0] = (if (F32::abs(fTemp43) > 1.1754944e-38) as i32 != 0 {fTemp43} else {0.0});
		let mut fTemp44: F32 = self.fRec21[1] + self.fRec12[1];
		let mut fTemp45: F32 = fRec29 + fRec22 + fRec14 + fRec20 + self.fRec24[1] + self.fRec30[1] + self.fRec23[1] + self.fRec15[1] - (fRec27 + fRec26 + fRec25 + fRec10 + self.fRec31[1] + self.fRec28[1] + fTemp44);
		self.fRec33[0] = (if (F32::abs(fTemp45) > 1.1754944e-38) as i32 != 0 {fTemp45} else {0.0});
		let mut fTemp46: F32 = self.fRec23[1] + fTemp44;
		let mut fTemp47: F32 = fRec27 + fRec26 + fRec29 + fRec22 + fRec25 + fRec10 + fRec14 + fRec20 + self.fRec31[1] + self.fRec28[1] + self.fRec24[1] + self.fRec30[1] + self.fRec15[1] + fTemp46;
		self.fRec34[0] = (if (F32::abs(fTemp47) > 1.1754944e-38) as i32 != 0 {fTemp47} else {0.0});
		let mut fTemp48: F32 = self.fRec21[1] + self.fRec23[1];
		let mut fTemp49: F32 = fRec25 + fRec10 + fRec14 + fRec20 + self.fRec28[1] + self.fRec15[1] + fTemp48 - (fRec27 + fRec26 + fRec29 + fRec22 + self.fRec31[1] + self.fRec24[1] + self.fRec12[1] + self.fRec30[1]);
		self.fRec35[0] = (if (F32::abs(fTemp49) > 1.1754944e-38) as i32 != 0 {fTemp49} else {0.0});
		let mut fTemp50: F32 = fRec27 + fRec22 + fRec25 + fRec20 + self.fRec31[1] + self.fRec28[1] + self.fRec15[1] + self.fRec30[1] - (fRec26 + fRec29 + fRec10 + fRec14 + self.fRec24[1] + fTemp46);
		self.fRec36[0] = (if (F32::abs(fTemp50) > 1.1754944e-38) as i32 != 0 {fTemp50} else {0.0});
		let mut fTemp51: F32 = fRec26 + fRec29 + fRec25 + fRec20 + self.fRec28[1] + self.fRec24[1] + self.fRec12[1] + self.fRec15[1] - (fRec27 + fRec22 + fRec10 + fRec14 + self.fRec31[1] + self.fRec30[1] + fTemp48);
		self.fRec37[0] = (if (F32::abs(fTemp51) > 1.1754944e-38) as i32 != 0 {fTemp51} else {0.0});
		let mut fTemp52: F32 = fRec26 + fRec22 + fRec10 + fRec20 + self.fRec30[1] + self.fRec15[1] + fTemp44 - (fRec27 + fRec29 + fRec25 + fRec14 + self.fRec31[1] + self.fRec28[1] + self.fRec23[1] + self.fRec24[1]);
		self.fRec38[0] = (if (F32::abs(fTemp52) > 1.1754944e-38) as i32 != 0 {fTemp52} else {0.0});
		let mut fTemp53: F32 = fRec27 + fRec29 + fRec10 + fRec20 + self.fRec31[1] + self.fRec24[1] + self.fRec21[1] + self.fRec15[1] - (fRec26 + fRec22 + fRec25 + fRec14 + self.fRec28[1] + self.fRec30[1] + fTemp42);
		self.fRec39[0] = (if (F32::abs(fTemp53) > 1.1754944e-38) as i32 != 0 {fTemp53} else {0.0});
		*output0 = (0.37 * (self.fRec35[0] + self.fRec33[0])) as FaustFloat;
		*output1 = (0.37 * (self.fRec35[0] - self.fRec33[0])) as FaustFloat;
		self.fRec0[1] = self.fRec0[0];
		self.fRec1[1] = self.fRec1[0];
		self.fRec2[1] = self.fRec2[0];
		self.fRec3[1] = self.fRec3[0];
		self.fRec4[1] = self.fRec4[0];
		self.fRec5[1] = self.fRec5[0];
		self.fRec6[1] = self.fRec6[0];
		self.fRec7[1] = self.fRec7[0];
		self.fRec8[1] = self.fRec8[0];
		self.fRec9[1] = self.fRec9[0];
		self.IOTA0 = i32::wrapping_add(self.IOTA0, 1);
		self.fRec11[1] = self.fRec11[0];
		self.fRec12[1] = self.fRec12[0];
		self.fRec13[1] = self.fRec13[0];
		self.fRec15[1] = self.fRec15[0];
		self.fRec16[1] = self.fRec16[0];
		self.fRec17[1] = self.fRec17[0];
		self.fRec18[1] = self.fRec18[0];
		self.fRec19[1] = self.fRec19[0];
		self.fRec21[1] = self.fRec21[0];
		self.fRec23[1] = self.fRec23[0];
		self.fRec24[1] = self.fRec24[0];
		self.fRec28[1] = self.fRec28[0];
		self.fRec30[1] = self.fRec30[0];
		self.fRec31[1] = self.fRec31[0];
		self.fRec32[2] = self.fRec32[1];
		self.fRec32[1] = self.fRec32[0];
		self.fRec33[2] = self.fRec33[1];
		self.fRec33[1] = self.fRec33[0];
		self.fRec34[2] = self.fRec34[1];
		self.fRec34[1] = self.fRec34[0];
		self.fRec35[2] = self.fRec35[1];
		self.fRec35[1] = self.fRec35[0];
		self.fRec36[2] = self.fRec36[1];
		self.fRec36[1] = self.fRec36[0];
		self.fRec37[2] = self.fRec37[1];
		self.fRec37[1] = self.fRec37[0];
		self.fRec38[2] = self.fRec38[1];
		self.fRec38[1] = self.fRec38[0];
		self.fRec39[2] = self.fRec39[1];
		self.fRec39[1] = self.fRec39[0];
		
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

