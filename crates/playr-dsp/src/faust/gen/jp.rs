/* ------------------------------------------------------------
name: "jp"
Code generated with Faust 2.88.0 (https://faust.grame.fr)
Compilation options: -lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1
------------------------------------------------------------ */

#[repr(C)]
pub struct Dsp {
	iVec0: [i32;2],
	IOTA0: i32,
	fVec1: [F32;16384],
	itbl0DspSIG0: [i32;2048],
	fSampleRate: i32,
	fConst0: F32,
	fConst1: F32,
	fHslider0: FaustFloat,
	fSlow0: F32,
	iSlow1: i32,
	fSlow2: F32,
	fRec62: [F32;2],
	fVec2: [F32;2],
	fRec0: [F32;2],
	fVec3: [F32;16384],
	iSlow3: i32,
	fSlow4: F32,
	fRec63: [F32;2],
	fVec4: [F32;2],
	fRec1: [F32;2],
	fVec5: [F32;16384],
	iSlow5: i32,
	fSlow6: F32,
	fRec64: [F32;2],
	fVec6: [F32;2],
	fRec2: [F32;2],
	fVec7: [F32;16384],
	iSlow7: i32,
	fSlow8: F32,
	fRec65: [F32;2],
	fVec8: [F32;2],
	fRec3: [F32;2],
	fVec9: [F32;16384],
	iSlow9: i32,
	fSlow10: F32,
	fRec66: [F32;2],
	fVec10: [F32;2],
	fRec4: [F32;2],
	fVec11: [F32;16384],
	iSlow11: i32,
	fSlow12: F32,
	fRec67: [F32;2],
	fVec12: [F32;2],
	fRec5: [F32;2],
	fVec13: [F32;16384],
	iSlow13: i32,
	fSlow14: F32,
	fRec68: [F32;2],
	fVec14: [F32;2],
	fRec6: [F32;2],
	fVec15: [F32;16384],
	iSlow15: i32,
	fSlow16: F32,
	fRec69: [F32;2],
	fVec16: [F32;2],
	fRec7: [F32;2],
	fVec17: [F32;16384],
	iSlow17: i32,
	fSlow18: F32,
	fRec70: [F32;2],
	fVec18: [F32;2],
	fRec8: [F32;2],
	fConst2: F32,
	fConst3: F32,
	fConst4: F32,
	fRec71: [F32;2],
	fRec72: [F32;2],
	fVec19: [F32;1024],
	fVec20: [F32;16384],
	iSlow19: i32,
	fSlow20: F32,
	fRec73: [F32;2],
	fVec21: [F32;2],
	fRec9: [F32;2],
	fVec22: [F32;16384],
	iSlow21: i32,
	fSlow22: F32,
	fRec74: [F32;2],
	fVec23: [F32;2],
	fRec10: [F32;2],
	fVec24: [F32;1024],
	fVec25: [F32;16384],
	iSlow23: i32,
	fSlow24: F32,
	fRec75: [F32;2],
	fVec26: [F32;2],
	fRec11: [F32;2],
	fVec27: [F32;16384],
	iSlow25: i32,
	fSlow26: F32,
	fRec76: [F32;2],
	fVec28: [F32;2],
	fRec12: [F32;2],
	fVec29: [F32;16384],
	iSlow27: i32,
	fSlow28: F32,
	fRec77: [F32;2],
	fVec30: [F32;2],
	fRec13: [F32;2],
	fVec31: [F32;16384],
	iSlow29: i32,
	fSlow30: F32,
	fRec78: [F32;2],
	fVec32: [F32;2],
	fRec14: [F32;2],
	fVec33: [F32;16384],
	iSlow31: i32,
	fSlow32: F32,
	fRec79: [F32;2],
	fVec34: [F32;2],
	fRec15: [F32;2],
	fVec35: [F32;16384],
	iSlow33: i32,
	fSlow34: F32,
	fRec80: [F32;2],
	fVec36: [F32;2],
	fRec16: [F32;2],
	fVec37: [F32;16384],
	iSlow35: i32,
	fSlow36: F32,
	fRec81: [F32;2],
	fVec38: [F32;2],
	fRec17: [F32;2],
	fVec39: [F32;16384],
	iSlow37: i32,
	fSlow38: F32,
	fRec82: [F32;2],
	fVec40: [F32;2],
	fRec18: [F32;2],
	fVec41: [F32;16384],
	iSlow39: i32,
	fSlow40: F32,
	fRec83: [F32;2],
	fVec42: [F32;2],
	fRec19: [F32;2],
	fVec43: [F32;16384],
	iSlow41: i32,
	fSlow42: F32,
	fRec84: [F32;2],
	fVec44: [F32;2],
	fRec20: [F32;2],
	fVec45: [F32;16384],
	iSlow43: i32,
	fSlow44: F32,
	fRec85: [F32;2],
	fVec46: [F32;2],
	fRec21: [F32;2],
	fVec47: [F32;16384],
	fVec48: [F32;16384],
	iSlow45: i32,
	fSlow46: F32,
	fRec86: [F32;2],
	fVec49: [F32;2],
	fRec22: [F32;2],
	fVec50: [F32;16384],
	fVec51: [F32;16384],
	iSlow47: i32,
	fSlow48: F32,
	fRec87: [F32;2],
	fVec52: [F32;2],
	fRec23: [F32;2],
	fConst5: F32,
	fConst6: F32,
	fConst7: F32,
	fConst8: F32,
	fRec24: [F32;2],
	fRec25: [F32;2],
	fRec26: [F32;2],
	fRec27: [F32;2],
	fConst9: F32,
	fConst10: F32,
	fConst11: F32,
	fConst12: F32,
	fConst13: F32,
	fRec28: [F32;3],
	fRec29: [F32;3],
	fRec30: [F32;3],
	fRec31: [F32;3],
	fConst14: F32,
	fConst15: F32,
	fConst16: F32,
	fRec32: [F32;3],
	fConst17: F32,
	fRec33: [F32;3],
	fRec34: [F32;3],
	fConst18: F32,
	fConst19: F32,
	fConst20: F32,
	fConst21: F32,
	fConst22: F32,
	fConst23: F32,
	fConst24: F32,
	fConst25: F32,
	fRec35: [F32;3],
	fRec36: [F32;3],
	fConst26: F32,
	fConst27: F32,
	fVec53: [F32;2],
	fRec37: [F32;2],
	fConst28: F32,
	fRec38: [F32;2],
	fRec39: [F32;3],
	fConst29: F32,
	fConst30: F32,
	fConst31: F32,
	fRec40: [F32;3],
	fRec41: [F32;3],
	fVec54: [F32;2],
	fRec42: [F32;2],
	fRec43: [F32;2],
	fConst32: F32,
	fConst33: F32,
	fRec44: [F32;3],
	fConst34: F32,
	fRec45: [F32;3],
	fRec46: [F32;3],
	fRec47: [F32;3],
	fRec48: [F32;3],
	fRec49: [F32;3],
	fHslider1: FaustFloat,
	fSlow49: F32,
	fSlow50: F32,
	fHslider2: FaustFloat,
	fSlow51: F32,
	fVec55: [F32;1024],
	fRec50: [F32;2],
	fVec56: [F32;1024],
	fRec51: [F32;2],
	fVec57: [F32;16384],
	fVec58: [F32;2],
	fRec52: [F32;2],
	fVec59: [F32;16384],
	iSlow52: i32,
	fSlow53: F32,
	fRec88: [F32;2],
	fVec60: [F32;2],
	fRec53: [F32;2],
	fVec61: [F32;16384],
	fVec62: [F32;2],
	fRec54: [F32;2],
	fVec63: [F32;16384],
	fVec64: [F32;2],
	fRec55: [F32;2],
	fVec65: [F32;16384],
	fVec66: [F32;2],
	fRec56: [F32;2],
	fVec67: [F32;16384],
	iSlow54: i32,
	fSlow55: F32,
	fRec89: [F32;2],
	fVec68: [F32;2],
	fRec57: [F32;2],
	fVec69: [F32;16384],
	fVec70: [F32;2],
	fRec58: [F32;2],
	fVec71: [F32;16384],
	iSlow56: i32,
	fSlow57: F32,
	fRec90: [F32;2],
	fVec72: [F32;2],
	fRec59: [F32;2],
	fRec60: [F32;2],
	fRec61: [F32;2],
	iDspSIG0Wave0_idx: i32,
}


fn Dsp_faustpower2_f(value: F32) -> F32 {
	return value * value;
}
static iDspSIG0Wave0: [i32;2048] = [2,3,5,7,11,13,17,19,23,29,31,37,41,43,47,53,59,61,67,71,73,79,83,89,97,101,103,107,109,113,127,131,137,139,149,151,157,163,167,173,179,181,191,193,197,199,211,223,227,229,233,239,241,251,257,263,269,271,277,281,283,293,307,311,313,317,331,337,347,349,353,359,367,373,379,383,389,397,401,409,419,421,431,433,439,443,449,457,461,463,467,479,487,491,499,503,509,521,523,541,547,557,563,569,571,577,587,593,599,601,607,613,617,619,631,641,643,647,653,659,661,673,677,683,691,701,709,719,727,733,739,743,751,757,761,769,773,787,797,809,811,821,823,827,829,839,853,857,859,863,877,881,883,887,907,911,919,929,937,941,947,953,967,971,977,983,991,997,1009,1013,1019,1021,1031,1033,1039,1049,1051,1061,1063,1069,1087,1091,1093,1097,1103,1109,1117,1123,1129,1151,1153,1163,1171,1181,1187,1193,1201,1213,1217,1223,1229,1231,1237,1249,1259,1277,1279,1283,1289,1291,1297,1301,1303,1307,1319,1321,1327,1361,1367,1373,1381,1399,1409,1423,1427,1429,1433,1439,1447,1451,1453,1459,1471,1481,1483,1487,1489,1493,1499,1511,1523,1531,1543,1549,1553,1559,1567,1571,1579,1583,1597,1601,1607,1609,1613,1619,1621,1627,1637,1657,1663,1667,1669,1693,1697,1699,1709,1721,1723,1733,1741,1747,1753,1759,1777,1783,1787,1789,1801,1811,1823,1831,1847,1861,1867,1871,1873,1877,1879,1889,1901,1907,1913,1931,1933,1949,1951,1973,1979,1987,1993,1997,1999,2003,2011,2017,2027,2029,2039,2053,2063,2069,2081,2083,2087,2089,2099,2111,2113,2129,2131,2137,2141,2143,2153,2161,2179,2203,2207,2213,2221,2237,2239,2243,2251,2267,2269,2273,2281,2287,2293,2297,2309,2311,2333,2339,2341,2347,2351,2357,2371,2377,2381,2383,2389,2393,2399,2411,2417,2423,2437,2441,2447,2459,2467,2473,2477,2503,2521,2531,2539,2543,2549,2551,2557,2579,2591,2593,2609,2617,2621,2633,2647,2657,2659,2663,2671,2677,2683,2687,2689,2693,2699,2707,2711,2713,2719,2729,2731,2741,2749,2753,2767,2777,2789,2791,2797,2801,2803,2819,2833,2837,2843,2851,2857,2861,2879,2887,2897,2903,2909,2917,2927,2939,2953,2957,2963,2969,2971,2999,3001,3011,3019,3023,3037,3041,3049,3061,3067,3079,3083,3089,3109,3119,3121,3137,3163,3167,3169,3181,3187,3191,3203,3209,3217,3221,3229,3251,3253,3257,3259,3271,3299,3301,3307,3313,3319,3323,3329,3331,3343,3347,3359,3361,3371,3373,3389,3391,3407,3413,3433,3449,3457,3461,3463,3467,3469,3491,3499,3511,3517,3527,3529,3533,3539,3541,3547,3557,3559,3571,3581,3583,3593,3607,3613,3617,3623,3631,3637,3643,3659,3671,3673,3677,3691,3697,3701,3709,3719,3727,3733,3739,3761,3767,3769,3779,3793,3797,3803,3821,3823,3833,3847,3851,3853,3863,3877,3881,3889,3907,3911,3917,3919,3923,3929,3931,3943,3947,3967,3989,4001,4003,4007,4013,4019,4021,4027,4049,4051,4057,4073,4079,4091,4093,4099,4111,4127,4129,4133,4139,4153,4157,4159,4177,4201,4211,4217,4219,4229,4231,4241,4243,4253,4259,4261,4271,4273,4283,4289,4297,4327,4337,4339,4349,4357,4363,4373,4391,4397,4409,4421,4423,4441,4447,4451,4457,4463,4481,4483,4493,4507,4513,4517,4519,4523,4547,4549,4561,4567,4583,4591,4597,4603,4621,4637,4639,4643,4649,4651,4657,4663,4673,4679,4691,4703,4721,4723,4729,4733,4751,4759,4783,4787,4789,4793,4799,4801,4813,4817,4831,4861,4871,4877,4889,4903,4909,4919,4931,4933,4937,4943,4951,4957,4967,4969,4973,4987,4993,4999,5003,5009,5011,5021,5023,5039,5051,5059,5077,5081,5087,5099,5101,5107,5113,5119,5147,5153,5167,5171,5179,5189,5197,5209,5227,5231,5233,5237,5261,5273,5279,5281,5297,5303,5309,5323,5333,5347,5351,5381,5387,5393,5399,5407,5413,5417,5419,5431,5437,5441,5443,5449,5471,5477,5479,5483,5501,5503,5507,5519,5521,5527,5531,5557,5563,5569,5573,5581,5591,5623,5639,5641,5647,5651,5653,5657,5659,5669,5683,5689,5693,5701,5711,5717,5737,5741,5743,5749,5779,5783,5791,5801,5807,5813,5821,5827,5839,5843,5849,5851,5857,5861,5867,5869,5879,5881,5897,5903,5923,5927,5939,5953,5981,5987,6007,6011,6029,6037,6043,6047,6053,6067,6073,6079,6089,6091,6101,6113,6121,6131,6133,6143,6151,6163,6173,6197,6199,6203,6211,6217,6221,6229,6247,6257,6263,6269,6271,6277,6287,6299,6301,6311,6317,6323,6329,6337,6343,6353,6359,6361,6367,6373,6379,6389,6397,6421,6427,6449,6451,6469,6473,6481,6491,6521,6529,6547,6551,6553,6563,6569,6571,6577,6581,6599,6607,6619,6637,6653,6659,6661,6673,6679,6689,6691,6701,6703,6709,6719,6733,6737,6761,6763,6779,6781,6791,6793,6803,6823,6827,6829,6833,6841,6857,6863,6869,6871,6883,6899,6907,6911,6917,6947,6949,6959,6961,6967,6971,6977,6983,6991,6997,7001,7013,7019,7027,7039,7043,7057,7069,7079,7103,7109,7121,7127,7129,7151,7159,7177,7187,7193,7207,7211,7213,7219,7229,7237,7243,7247,7253,7283,7297,7307,7309,7321,7331,7333,7349,7351,7369,7393,7411,7417,7433,7451,7457,7459,7477,7481,7487,7489,7499,7507,7517,7523,7529,7537,7541,7547,7549,7559,7561,7573,7577,7583,7589,7591,7603,7607,7621,7639,7643,7649,7669,7673,7681,7687,7691,7699,7703,7717,7723,7727,7741,7753,7757,7759,7789,7793,7817,7823,7829,7841,7853,7867,7873,7877,7879,7883,7901,7907,7919,7927,7933,7937,7949,7951,7963,7993,8009,8011,8017,8039,8053,8059,8069,8081,8087,8089,8093,8101,8111,8117,8123,8147,8161,8167,8171,8179,8191,8209,8219,8221,8231,8233,8237,8243,8263,8269,8273,8287,8291,8293,8297,8311,8317,8329,8353,8363,8369,8377,8387,8389,8419,8423,8429,8431,8443,8447,8461,8467,8501,8513,8521,8527,8537,8539,8543,8563,8573,8581,8597,8599,8609,8623,8627,8629,8641,8647,8663,8669,8677,8681,8689,8693,8699,8707,8713,8719,8731,8737,8741,8747,8753,8761,8779,8783,8803,8807,8819,8821,8831,8837,8839,8849,8861,8863,8867,8887,8893,8923,8929,8933,8941,8951,8963,8969,8971,8999,9001,9007,9011,9013,9029,9041,9043,9049,9059,9067,9091,9103,9109,9127,9133,9137,9151,9157,9161,9173,9181,9187,9199,9203,9209,9221,9227,9239,9241,9257,9277,9281,9283,9293,9311,9319,9323,9337,9341,9343,9349,9371,9377,9391,9397,9403,9413,9419,9421,9431,9433,9437,9439,9461,9463,9467,9473,9479,9491,9497,9511,9521,9533,9539,9547,9551,9587,9601,9613,9619,9623,9629,9631,9643,9649,9661,9677,9679,9689,9697,9719,9721,9733,9739,9743,9749,9767,9769,9781,9787,9791,9803,9811,9817,9829,9833,9839,9851,9857,9859,9871,9883,9887,9901,9907,9923,9929,9931,9941,9949,9967,9973,10007,10009,10037,10039,10061,10067,10069,10079,10091,10093,10099,10103,10111,10133,10139,10141,10151,10159,10163,10169,10177,10181,10193,10211,10223,10243,10247,10253,10259,10267,10271,10273,10289,10301,10303,10313,10321,10331,10333,10337,10343,10357,10369,10391,10399,10427,10429,10433,10453,10457,10459,10463,10477,10487,10499,10501,10513,10529,10531,10559,10567,10589,10597,10601,10607,10613,10627,10631,10639,10651,10657,10663,10667,10687,10691,10709,10711,10723,10729,10733,10739,10753,10771,10781,10789,10799,10831,10837,10847,10853,10859,10861,10867,10883,10889,10891,10903,10909,10937,10939,10949,10957,10973,10979,10987,10993,11003,11027,11047,11057,11059,11069,11071,11083,11087,11093,11113,11117,11119,11131,11149,11159,11161,11171,11173,11177,11197,11213,11239,11243,11251,11257,11261,11273,11279,11287,11299,11311,11317,11321,11329,11351,11353,11369,11383,11393,11399,11411,11423,11437,11443,11447,11467,11471,11483,11489,11491,11497,11503,11519,11527,11549,11551,11579,11587,11593,11597,11617,11621,11633,11657,11677,11681,11689,11699,11701,11717,11719,11731,11743,11777,11779,11783,11789,11801,11807,11813,11821,11827,11831,11833,11839,11863,11867,11887,11897,11903,11909,11923,11927,11933,11939,11941,11953,11959,11969,11971,11981,11987,12007,12011,12037,12041,12043,12049,12071,12073,12097,12101,12107,12109,12113,12119,12143,12149,12157,12161,12163,12197,12203,12211,12227,12239,12241,12251,12253,12263,12269,12277,12281,12289,12301,12323,12329,12343,12347,12373,12377,12379,12391,12401,12409,12413,12421,12433,12437,12451,12457,12473,12479,12487,12491,12497,12503,12511,12517,12527,12539,12541,12547,12553,12569,12577,12583,12589,12601,12611,12613,12619,12637,12641,12647,12653,12659,12671,12689,12697,12703,12713,12721,12739,12743,12757,12763,12781,12791,12799,12809,12821,12823,12829,12841,12853,12889,12893,12899,12907,12911,12917,12919,12923,12941,12953,12959,12967,12973,12979,12983,13001,13003,13007,13009,13033,13037,13043,13049,13063,13093,13099,13103,13109,13121,13127,13147,13151,13159,13163,13171,13177,13183,13187,13217,13219,13229,13241,13249,13259,13267,13291,13297,13309,13313,13327,13331,13337,13339,13367,13381,13397,13399,13411,13417,13421,13441,13451,13457,13463,13469,13477,13487,13499,13513,13523,13537,13553,13567,13577,13591,13597,13613,13619,13627,13633,13649,13669,13679,13681,13687,13691,13693,13697,13709,13711,13721,13723,13729,13751,13757,13759,13763,13781,13789,13799,13807,13829,13831,13841,13859,13873,13877,13879,13883,13901,13903,13907,13913,13921,13931,13933,13963,13967,13997,13999,14009,14011,14029,14033,14051,14057,14071,14081,14083,14087,14107,14143,14149,14153,14159,14173,14177,14197,14207,14221,14243,14249,14251,14281,14293,14303,14321,14323,14327,14341,14347,14369,14387,14389,14401,14407,14411,14419,14423,14431,14437,14447,14449,14461,14479,14489,14503,14519,14533,14537,14543,14549,14551,14557,14561,14563,14591,14593,14621,14627,14629,14633,14639,14653,14657,14669,14683,14699,14713,14717,14723,14731,14737,14741,14747,14753,14759,14767,14771,14779,14783,14797,14813,14821,14827,14831,14843,14851,14867,14869,14879,14887,14891,14897,14923,14929,14939,14947,14951,14957,14969,14983,15013,15017,15031,15053,15061,15073,15077,15083,15091,15101,15107,15121,15131,15137,15139,15149,15161,15173,15187,15193,15199,15217,15227,15233,15241,15259,15263,15269,15271,15277,15287,15289,15299,15307,15313,15319,15329,15331,15349,15359,15361,15373,15377,15383,15391,15401,15413,15427,15439,15443,15451,15461,15467,15473,15493,15497,15511,15527,15541,15551,15559,15569,15581,15583,15601,15607,15619,15629,15641,15643,15647,15649,15661,15667,15671,15679,15683,15727,15731,15733,15737,15739,15749,15761,15767,15773,15787,15791,15797,15803,15809,15817,15823,15859,15877,15881,15887,15889,15901,15907,15913,15919,15923,15937,15959,15971,15973,15991,16001,16007,16033,16057,16061,16063,16067,16069,16073,16087,16091,16097,16103,16111,16127,16139,16141,16183,16187,16189,16193,16217,16223,16229,16231,16249,16253,16267,16273,16301,16319,16333,16339,16349,16361,16363,16369,16381,16411,16417,16421,16427,16433,16447,16451,16453,16477,16481,16487,16493,16519,16529,16547,16553,16561,16567,16573,16603,16607,16619,16631,16633,16649,16651,16657,16661,16673,16691,16693,16699,16703,16729,16741,16747,16759,16763,16787,16811,16823,16829,16831,16843,16871,16879,16883,16889,16901,16903,16921,16927,16931,16937,16943,16963,16979,16981,16987,16993,17011,17021,17027,17029,17033,17041,17047,17053,17077,17093,17099,17107,17117,17123,17137,17159,17167,17183,17189,17191,17203,17207,17209,17231,17239,17257,17291,17293,17299,17317,17321,17327,17333,17341,17351,17359,17377,17383,17387,17389,17393,17401,17417,17419,17431,17443,17449,17467,17471,17477,17483,17489,17491,17497,17509,17519,17539,17551,17569,17573,17579,17581,17597,17599,17609,17623,17627,17657,17659,17669,17681,17683,17707,17713,17729,17737,17747,17749,17761,17783,17789,17791,17807,17827,17837,17839,17851,17863];
pub const FAUST_INPUTS: usize = 2;
pub const FAUST_OUTPUTS: usize = 2;
pub const FAUST_ACTIVES: usize = 3;
pub const FAUST_PASSIVES: usize = 0;

impl Dsp {
		
	pub fn new() -> Dsp { 
		Dsp {
			iVec0: [0;2],
			IOTA0: 0,
			fVec1: [0.0;16384],
			itbl0DspSIG0: [0;2048],
			fSampleRate: 0,
			fConst0: 0.0,
			fConst1: 0.0,
			fHslider0: 0.0,
			fSlow0: 0.0,
			iSlow1: 0,
			fSlow2: 0.0,
			fRec62: [0.0;2],
			fVec2: [0.0;2],
			fRec0: [0.0;2],
			fVec3: [0.0;16384],
			iSlow3: 0,
			fSlow4: 0.0,
			fRec63: [0.0;2],
			fVec4: [0.0;2],
			fRec1: [0.0;2],
			fVec5: [0.0;16384],
			iSlow5: 0,
			fSlow6: 0.0,
			fRec64: [0.0;2],
			fVec6: [0.0;2],
			fRec2: [0.0;2],
			fVec7: [0.0;16384],
			iSlow7: 0,
			fSlow8: 0.0,
			fRec65: [0.0;2],
			fVec8: [0.0;2],
			fRec3: [0.0;2],
			fVec9: [0.0;16384],
			iSlow9: 0,
			fSlow10: 0.0,
			fRec66: [0.0;2],
			fVec10: [0.0;2],
			fRec4: [0.0;2],
			fVec11: [0.0;16384],
			iSlow11: 0,
			fSlow12: 0.0,
			fRec67: [0.0;2],
			fVec12: [0.0;2],
			fRec5: [0.0;2],
			fVec13: [0.0;16384],
			iSlow13: 0,
			fSlow14: 0.0,
			fRec68: [0.0;2],
			fVec14: [0.0;2],
			fRec6: [0.0;2],
			fVec15: [0.0;16384],
			iSlow15: 0,
			fSlow16: 0.0,
			fRec69: [0.0;2],
			fVec16: [0.0;2],
			fRec7: [0.0;2],
			fVec17: [0.0;16384],
			iSlow17: 0,
			fSlow18: 0.0,
			fRec70: [0.0;2],
			fVec18: [0.0;2],
			fRec8: [0.0;2],
			fConst2: 0.0,
			fConst3: 0.0,
			fConst4: 0.0,
			fRec71: [0.0;2],
			fRec72: [0.0;2],
			fVec19: [0.0;1024],
			fVec20: [0.0;16384],
			iSlow19: 0,
			fSlow20: 0.0,
			fRec73: [0.0;2],
			fVec21: [0.0;2],
			fRec9: [0.0;2],
			fVec22: [0.0;16384],
			iSlow21: 0,
			fSlow22: 0.0,
			fRec74: [0.0;2],
			fVec23: [0.0;2],
			fRec10: [0.0;2],
			fVec24: [0.0;1024],
			fVec25: [0.0;16384],
			iSlow23: 0,
			fSlow24: 0.0,
			fRec75: [0.0;2],
			fVec26: [0.0;2],
			fRec11: [0.0;2],
			fVec27: [0.0;16384],
			iSlow25: 0,
			fSlow26: 0.0,
			fRec76: [0.0;2],
			fVec28: [0.0;2],
			fRec12: [0.0;2],
			fVec29: [0.0;16384],
			iSlow27: 0,
			fSlow28: 0.0,
			fRec77: [0.0;2],
			fVec30: [0.0;2],
			fRec13: [0.0;2],
			fVec31: [0.0;16384],
			iSlow29: 0,
			fSlow30: 0.0,
			fRec78: [0.0;2],
			fVec32: [0.0;2],
			fRec14: [0.0;2],
			fVec33: [0.0;16384],
			iSlow31: 0,
			fSlow32: 0.0,
			fRec79: [0.0;2],
			fVec34: [0.0;2],
			fRec15: [0.0;2],
			fVec35: [0.0;16384],
			iSlow33: 0,
			fSlow34: 0.0,
			fRec80: [0.0;2],
			fVec36: [0.0;2],
			fRec16: [0.0;2],
			fVec37: [0.0;16384],
			iSlow35: 0,
			fSlow36: 0.0,
			fRec81: [0.0;2],
			fVec38: [0.0;2],
			fRec17: [0.0;2],
			fVec39: [0.0;16384],
			iSlow37: 0,
			fSlow38: 0.0,
			fRec82: [0.0;2],
			fVec40: [0.0;2],
			fRec18: [0.0;2],
			fVec41: [0.0;16384],
			iSlow39: 0,
			fSlow40: 0.0,
			fRec83: [0.0;2],
			fVec42: [0.0;2],
			fRec19: [0.0;2],
			fVec43: [0.0;16384],
			iSlow41: 0,
			fSlow42: 0.0,
			fRec84: [0.0;2],
			fVec44: [0.0;2],
			fRec20: [0.0;2],
			fVec45: [0.0;16384],
			iSlow43: 0,
			fSlow44: 0.0,
			fRec85: [0.0;2],
			fVec46: [0.0;2],
			fRec21: [0.0;2],
			fVec47: [0.0;16384],
			fVec48: [0.0;16384],
			iSlow45: 0,
			fSlow46: 0.0,
			fRec86: [0.0;2],
			fVec49: [0.0;2],
			fRec22: [0.0;2],
			fVec50: [0.0;16384],
			fVec51: [0.0;16384],
			iSlow47: 0,
			fSlow48: 0.0,
			fRec87: [0.0;2],
			fVec52: [0.0;2],
			fRec23: [0.0;2],
			fConst5: 0.0,
			fConst6: 0.0,
			fConst7: 0.0,
			fConst8: 0.0,
			fRec24: [0.0;2],
			fRec25: [0.0;2],
			fRec26: [0.0;2],
			fRec27: [0.0;2],
			fConst9: 0.0,
			fConst10: 0.0,
			fConst11: 0.0,
			fConst12: 0.0,
			fConst13: 0.0,
			fRec28: [0.0;3],
			fRec29: [0.0;3],
			fRec30: [0.0;3],
			fRec31: [0.0;3],
			fConst14: 0.0,
			fConst15: 0.0,
			fConst16: 0.0,
			fRec32: [0.0;3],
			fConst17: 0.0,
			fRec33: [0.0;3],
			fRec34: [0.0;3],
			fConst18: 0.0,
			fConst19: 0.0,
			fConst20: 0.0,
			fConst21: 0.0,
			fConst22: 0.0,
			fConst23: 0.0,
			fConst24: 0.0,
			fConst25: 0.0,
			fRec35: [0.0;3],
			fRec36: [0.0;3],
			fConst26: 0.0,
			fConst27: 0.0,
			fVec53: [0.0;2],
			fRec37: [0.0;2],
			fConst28: 0.0,
			fRec38: [0.0;2],
			fRec39: [0.0;3],
			fConst29: 0.0,
			fConst30: 0.0,
			fConst31: 0.0,
			fRec40: [0.0;3],
			fRec41: [0.0;3],
			fVec54: [0.0;2],
			fRec42: [0.0;2],
			fRec43: [0.0;2],
			fConst32: 0.0,
			fConst33: 0.0,
			fRec44: [0.0;3],
			fConst34: 0.0,
			fRec45: [0.0;3],
			fRec46: [0.0;3],
			fRec47: [0.0;3],
			fRec48: [0.0;3],
			fRec49: [0.0;3],
			fHslider1: 0.0,
			fSlow49: 0.0,
			fSlow50: 0.0,
			fHslider2: 0.0,
			fSlow51: 0.0,
			fVec55: [0.0;1024],
			fRec50: [0.0;2],
			fVec56: [0.0;1024],
			fRec51: [0.0;2],
			fVec57: [0.0;16384],
			fVec58: [0.0;2],
			fRec52: [0.0;2],
			fVec59: [0.0;16384],
			iSlow52: 0,
			fSlow53: 0.0,
			fRec88: [0.0;2],
			fVec60: [0.0;2],
			fRec53: [0.0;2],
			fVec61: [0.0;16384],
			fVec62: [0.0;2],
			fRec54: [0.0;2],
			fVec63: [0.0;16384],
			fVec64: [0.0;2],
			fRec55: [0.0;2],
			fVec65: [0.0;16384],
			fVec66: [0.0;2],
			fRec56: [0.0;2],
			fVec67: [0.0;16384],
			iSlow54: 0,
			fSlow55: 0.0,
			fRec89: [0.0;2],
			fVec68: [0.0;2],
			fRec57: [0.0;2],
			fVec69: [0.0;16384],
			fVec70: [0.0;2],
			fRec58: [0.0;2],
			fVec71: [0.0;16384],
			iSlow56: 0,
			fSlow57: 0.0,
			fRec90: [0.0;2],
			fVec72: [0.0;2],
			fRec59: [0.0;2],
			fRec60: [0.0;2],
			fRec61: [0.0;2],
			iDspSIG0Wave0_idx: 0,
		}
	}
	pub fn metadata(&self, m: &mut dyn Meta) { 
		m.declare("analyzers.lib/name", r"Faust Analyzer Library");
		m.declare("analyzers.lib/version", r"1.4.0");
		m.declare("basics.lib/name", r"Faust Basic Element Library");
		m.declare("basics.lib/version", r"1.23.0");
		m.declare("compile_options", r"-lang rust -fpga-mem-th 4 -os -it -ec -ct 1 -cn Dsp -es 1 -mcd 16 -mdd 1024 -mdy 33 -single -ftz 1");
		m.declare("delays.lib/fdelay1a:author", r"Julius O. Smith III");
		m.declare("delays.lib/fdelay4:author", r"Julius O. Smith III");
		m.declare("delays.lib/fdelayltv:author", r"Julius O. Smith III");
		m.declare("delays.lib/name", r"Faust Delay Library");
		m.declare("delays.lib/version", r"1.2.0");
		m.declare("filename", r"jp.dsp");
		m.declare("filters.lib/filterbank:author", r"Julius O. Smith III");
		m.declare("filters.lib/filterbank:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/filterbank:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/fir:author", r"Julius O. Smith III");
		m.declare("filters.lib/fir:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/fir:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/highpass:author", r"Julius O. Smith III");
		m.declare("filters.lib/highpass:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/highpass:license", r"LicenseRef-STK-4.3");
		m.declare("filters.lib/highpass_plus_lowpass:author", r"Julius O. Smith III");
		m.declare("filters.lib/highpass_plus_lowpass:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/highpass_plus_lowpass:license", r"LicenseRef-STK-4.3");
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
		m.declare("filters.lib/nlf2:author", r"Julius O. Smith III");
		m.declare("filters.lib/nlf2:copyright", r"Copyright (C) 2003-2019 by Julius O. Smith III <jos@ccrma.stanford.edu>");
		m.declare("filters.lib/nlf2:license", r"LicenseRef-STK-4.3");
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
		m.declare("maths.lib/author", r"GRAME");
		m.declare("maths.lib/copyright", r"GRAME");
		m.declare("maths.lib/license", r"LicenseRef-LGPL-2.1-or-later-with-Faust-exception");
		m.declare("maths.lib/name", r"Faust Math Library");
		m.declare("maths.lib/version", r"2.9.0");
		m.declare("name", r"jp");
		m.declare("oscillators.lib/name", r"Faust Oscillator Library");
		m.declare("oscillators.lib/version", r"1.8.0");
		m.declare("platform.lib/name", r"Generic Platform Library");
		m.declare("platform.lib/version", r"1.3.0");
		m.declare("reverbs.lib/jpverb:author", r"Julian Parker, bug fixes and minor interface changes by Till Bovermann");
		m.declare("reverbs.lib/jpverb:license", r"MIT");
		m.declare("reverbs.lib/name", r"Faust Reverb Library");
		m.declare("reverbs.lib/version", r"1.5.1");
		m.declare("signals.lib/name", r"Faust Routing Library");
		m.declare("signals.lib/version", r"1.7.0");
	}

	pub fn get_sample_rate(&self) -> i32 { self.fSampleRate as i32}
	pub fn class_init(sample_rate: i32) {}
	
	pub fn static_init(&mut self,sample_rate: i32) {
		self.iDspSIG0Wave0_idx = 0;
		for i1_re0 in 0..2048 {
			self.itbl0DspSIG0[i1_re0 as usize] = iDspSIG0Wave0[self.iDspSIG0Wave0_idx as usize];
			self.iDspSIG0Wave0_idx = (i32::wrapping_add(1, self.iDspSIG0Wave0_idx)) % 2048;
		}
	}
	pub fn instance_reset_params(&mut self) {
		self.fHslider0 = (1.0) as FaustFloat;
		self.fHslider1 = (0.3) as FaustFloat;
		self.fHslider2 = (2.0) as FaustFloat;
	}
	pub fn instance_clear(&mut self) {
		for l0 in 0..2 {
			self.iVec0[l0 as usize] = 0;
		}
		self.IOTA0 = 0;
		for l1 in 0..16384 {
			self.fVec1[l1 as usize] = 0.0;
		}
		for l2 in 0..2 {
			self.fRec62[l2 as usize] = 0.0;
		}
		for l3 in 0..2 {
			self.fVec2[l3 as usize] = 0.0;
		}
		for l4 in 0..2 {
			self.fRec0[l4 as usize] = 0.0;
		}
		for l5 in 0..16384 {
			self.fVec3[l5 as usize] = 0.0;
		}
		for l6 in 0..2 {
			self.fRec63[l6 as usize] = 0.0;
		}
		for l7 in 0..2 {
			self.fVec4[l7 as usize] = 0.0;
		}
		for l8 in 0..2 {
			self.fRec1[l8 as usize] = 0.0;
		}
		for l9 in 0..16384 {
			self.fVec5[l9 as usize] = 0.0;
		}
		for l10 in 0..2 {
			self.fRec64[l10 as usize] = 0.0;
		}
		for l11 in 0..2 {
			self.fVec6[l11 as usize] = 0.0;
		}
		for l12 in 0..2 {
			self.fRec2[l12 as usize] = 0.0;
		}
		for l13 in 0..16384 {
			self.fVec7[l13 as usize] = 0.0;
		}
		for l14 in 0..2 {
			self.fRec65[l14 as usize] = 0.0;
		}
		for l15 in 0..2 {
			self.fVec8[l15 as usize] = 0.0;
		}
		for l16 in 0..2 {
			self.fRec3[l16 as usize] = 0.0;
		}
		for l17 in 0..16384 {
			self.fVec9[l17 as usize] = 0.0;
		}
		for l18 in 0..2 {
			self.fRec66[l18 as usize] = 0.0;
		}
		for l19 in 0..2 {
			self.fVec10[l19 as usize] = 0.0;
		}
		for l20 in 0..2 {
			self.fRec4[l20 as usize] = 0.0;
		}
		for l21 in 0..16384 {
			self.fVec11[l21 as usize] = 0.0;
		}
		for l22 in 0..2 {
			self.fRec67[l22 as usize] = 0.0;
		}
		for l23 in 0..2 {
			self.fVec12[l23 as usize] = 0.0;
		}
		for l24 in 0..2 {
			self.fRec5[l24 as usize] = 0.0;
		}
		for l25 in 0..16384 {
			self.fVec13[l25 as usize] = 0.0;
		}
		for l26 in 0..2 {
			self.fRec68[l26 as usize] = 0.0;
		}
		for l27 in 0..2 {
			self.fVec14[l27 as usize] = 0.0;
		}
		for l28 in 0..2 {
			self.fRec6[l28 as usize] = 0.0;
		}
		for l29 in 0..16384 {
			self.fVec15[l29 as usize] = 0.0;
		}
		for l30 in 0..2 {
			self.fRec69[l30 as usize] = 0.0;
		}
		for l31 in 0..2 {
			self.fVec16[l31 as usize] = 0.0;
		}
		for l32 in 0..2 {
			self.fRec7[l32 as usize] = 0.0;
		}
		for l33 in 0..16384 {
			self.fVec17[l33 as usize] = 0.0;
		}
		for l34 in 0..2 {
			self.fRec70[l34 as usize] = 0.0;
		}
		for l35 in 0..2 {
			self.fVec18[l35 as usize] = 0.0;
		}
		for l36 in 0..2 {
			self.fRec8[l36 as usize] = 0.0;
		}
		for l37 in 0..2 {
			self.fRec71[l37 as usize] = 0.0;
		}
		for l38 in 0..2 {
			self.fRec72[l38 as usize] = 0.0;
		}
		for l39 in 0..1024 {
			self.fVec19[l39 as usize] = 0.0;
		}
		for l40 in 0..16384 {
			self.fVec20[l40 as usize] = 0.0;
		}
		for l41 in 0..2 {
			self.fRec73[l41 as usize] = 0.0;
		}
		for l42 in 0..2 {
			self.fVec21[l42 as usize] = 0.0;
		}
		for l43 in 0..2 {
			self.fRec9[l43 as usize] = 0.0;
		}
		for l44 in 0..16384 {
			self.fVec22[l44 as usize] = 0.0;
		}
		for l45 in 0..2 {
			self.fRec74[l45 as usize] = 0.0;
		}
		for l46 in 0..2 {
			self.fVec23[l46 as usize] = 0.0;
		}
		for l47 in 0..2 {
			self.fRec10[l47 as usize] = 0.0;
		}
		for l48 in 0..1024 {
			self.fVec24[l48 as usize] = 0.0;
		}
		for l49 in 0..16384 {
			self.fVec25[l49 as usize] = 0.0;
		}
		for l50 in 0..2 {
			self.fRec75[l50 as usize] = 0.0;
		}
		for l51 in 0..2 {
			self.fVec26[l51 as usize] = 0.0;
		}
		for l52 in 0..2 {
			self.fRec11[l52 as usize] = 0.0;
		}
		for l53 in 0..16384 {
			self.fVec27[l53 as usize] = 0.0;
		}
		for l54 in 0..2 {
			self.fRec76[l54 as usize] = 0.0;
		}
		for l55 in 0..2 {
			self.fVec28[l55 as usize] = 0.0;
		}
		for l56 in 0..2 {
			self.fRec12[l56 as usize] = 0.0;
		}
		for l57 in 0..16384 {
			self.fVec29[l57 as usize] = 0.0;
		}
		for l58 in 0..2 {
			self.fRec77[l58 as usize] = 0.0;
		}
		for l59 in 0..2 {
			self.fVec30[l59 as usize] = 0.0;
		}
		for l60 in 0..2 {
			self.fRec13[l60 as usize] = 0.0;
		}
		for l61 in 0..16384 {
			self.fVec31[l61 as usize] = 0.0;
		}
		for l62 in 0..2 {
			self.fRec78[l62 as usize] = 0.0;
		}
		for l63 in 0..2 {
			self.fVec32[l63 as usize] = 0.0;
		}
		for l64 in 0..2 {
			self.fRec14[l64 as usize] = 0.0;
		}
		for l65 in 0..16384 {
			self.fVec33[l65 as usize] = 0.0;
		}
		for l66 in 0..2 {
			self.fRec79[l66 as usize] = 0.0;
		}
		for l67 in 0..2 {
			self.fVec34[l67 as usize] = 0.0;
		}
		for l68 in 0..2 {
			self.fRec15[l68 as usize] = 0.0;
		}
		for l69 in 0..16384 {
			self.fVec35[l69 as usize] = 0.0;
		}
		for l70 in 0..2 {
			self.fRec80[l70 as usize] = 0.0;
		}
		for l71 in 0..2 {
			self.fVec36[l71 as usize] = 0.0;
		}
		for l72 in 0..2 {
			self.fRec16[l72 as usize] = 0.0;
		}
		for l73 in 0..16384 {
			self.fVec37[l73 as usize] = 0.0;
		}
		for l74 in 0..2 {
			self.fRec81[l74 as usize] = 0.0;
		}
		for l75 in 0..2 {
			self.fVec38[l75 as usize] = 0.0;
		}
		for l76 in 0..2 {
			self.fRec17[l76 as usize] = 0.0;
		}
		for l77 in 0..16384 {
			self.fVec39[l77 as usize] = 0.0;
		}
		for l78 in 0..2 {
			self.fRec82[l78 as usize] = 0.0;
		}
		for l79 in 0..2 {
			self.fVec40[l79 as usize] = 0.0;
		}
		for l80 in 0..2 {
			self.fRec18[l80 as usize] = 0.0;
		}
		for l81 in 0..16384 {
			self.fVec41[l81 as usize] = 0.0;
		}
		for l82 in 0..2 {
			self.fRec83[l82 as usize] = 0.0;
		}
		for l83 in 0..2 {
			self.fVec42[l83 as usize] = 0.0;
		}
		for l84 in 0..2 {
			self.fRec19[l84 as usize] = 0.0;
		}
		for l85 in 0..16384 {
			self.fVec43[l85 as usize] = 0.0;
		}
		for l86 in 0..2 {
			self.fRec84[l86 as usize] = 0.0;
		}
		for l87 in 0..2 {
			self.fVec44[l87 as usize] = 0.0;
		}
		for l88 in 0..2 {
			self.fRec20[l88 as usize] = 0.0;
		}
		for l89 in 0..16384 {
			self.fVec45[l89 as usize] = 0.0;
		}
		for l90 in 0..2 {
			self.fRec85[l90 as usize] = 0.0;
		}
		for l91 in 0..2 {
			self.fVec46[l91 as usize] = 0.0;
		}
		for l92 in 0..2 {
			self.fRec21[l92 as usize] = 0.0;
		}
		for l93 in 0..16384 {
			self.fVec47[l93 as usize] = 0.0;
		}
		for l94 in 0..16384 {
			self.fVec48[l94 as usize] = 0.0;
		}
		for l95 in 0..2 {
			self.fRec86[l95 as usize] = 0.0;
		}
		for l96 in 0..2 {
			self.fVec49[l96 as usize] = 0.0;
		}
		for l97 in 0..2 {
			self.fRec22[l97 as usize] = 0.0;
		}
		for l98 in 0..16384 {
			self.fVec50[l98 as usize] = 0.0;
		}
		for l99 in 0..16384 {
			self.fVec51[l99 as usize] = 0.0;
		}
		for l100 in 0..2 {
			self.fRec87[l100 as usize] = 0.0;
		}
		for l101 in 0..2 {
			self.fVec52[l101 as usize] = 0.0;
		}
		for l102 in 0..2 {
			self.fRec23[l102 as usize] = 0.0;
		}
		for l103 in 0..2 {
			self.fRec24[l103 as usize] = 0.0;
		}
		for l104 in 0..2 {
			self.fRec25[l104 as usize] = 0.0;
		}
		for l105 in 0..2 {
			self.fRec26[l105 as usize] = 0.0;
		}
		for l106 in 0..2 {
			self.fRec27[l106 as usize] = 0.0;
		}
		for l107 in 0..3 {
			self.fRec28[l107 as usize] = 0.0;
		}
		for l108 in 0..3 {
			self.fRec29[l108 as usize] = 0.0;
		}
		for l109 in 0..3 {
			self.fRec30[l109 as usize] = 0.0;
		}
		for l110 in 0..3 {
			self.fRec31[l110 as usize] = 0.0;
		}
		for l111 in 0..3 {
			self.fRec32[l111 as usize] = 0.0;
		}
		for l112 in 0..3 {
			self.fRec33[l112 as usize] = 0.0;
		}
		for l113 in 0..3 {
			self.fRec34[l113 as usize] = 0.0;
		}
		for l114 in 0..3 {
			self.fRec35[l114 as usize] = 0.0;
		}
		for l115 in 0..3 {
			self.fRec36[l115 as usize] = 0.0;
		}
		for l116 in 0..2 {
			self.fVec53[l116 as usize] = 0.0;
		}
		for l117 in 0..2 {
			self.fRec37[l117 as usize] = 0.0;
		}
		for l118 in 0..2 {
			self.fRec38[l118 as usize] = 0.0;
		}
		for l119 in 0..3 {
			self.fRec39[l119 as usize] = 0.0;
		}
		for l120 in 0..3 {
			self.fRec40[l120 as usize] = 0.0;
		}
		for l121 in 0..3 {
			self.fRec41[l121 as usize] = 0.0;
		}
		for l122 in 0..2 {
			self.fVec54[l122 as usize] = 0.0;
		}
		for l123 in 0..2 {
			self.fRec42[l123 as usize] = 0.0;
		}
		for l124 in 0..2 {
			self.fRec43[l124 as usize] = 0.0;
		}
		for l125 in 0..3 {
			self.fRec44[l125 as usize] = 0.0;
		}
		for l126 in 0..3 {
			self.fRec45[l126 as usize] = 0.0;
		}
		for l127 in 0..3 {
			self.fRec46[l127 as usize] = 0.0;
		}
		for l128 in 0..3 {
			self.fRec47[l128 as usize] = 0.0;
		}
		for l129 in 0..3 {
			self.fRec48[l129 as usize] = 0.0;
		}
		for l130 in 0..3 {
			self.fRec49[l130 as usize] = 0.0;
		}
		for l131 in 0..1024 {
			self.fVec55[l131 as usize] = 0.0;
		}
		for l132 in 0..2 {
			self.fRec50[l132 as usize] = 0.0;
		}
		for l133 in 0..1024 {
			self.fVec56[l133 as usize] = 0.0;
		}
		for l134 in 0..2 {
			self.fRec51[l134 as usize] = 0.0;
		}
		for l135 in 0..16384 {
			self.fVec57[l135 as usize] = 0.0;
		}
		for l136 in 0..2 {
			self.fVec58[l136 as usize] = 0.0;
		}
		for l137 in 0..2 {
			self.fRec52[l137 as usize] = 0.0;
		}
		for l138 in 0..16384 {
			self.fVec59[l138 as usize] = 0.0;
		}
		for l139 in 0..2 {
			self.fRec88[l139 as usize] = 0.0;
		}
		for l140 in 0..2 {
			self.fVec60[l140 as usize] = 0.0;
		}
		for l141 in 0..2 {
			self.fRec53[l141 as usize] = 0.0;
		}
		for l142 in 0..16384 {
			self.fVec61[l142 as usize] = 0.0;
		}
		for l143 in 0..2 {
			self.fVec62[l143 as usize] = 0.0;
		}
		for l144 in 0..2 {
			self.fRec54[l144 as usize] = 0.0;
		}
		for l145 in 0..16384 {
			self.fVec63[l145 as usize] = 0.0;
		}
		for l146 in 0..2 {
			self.fVec64[l146 as usize] = 0.0;
		}
		for l147 in 0..2 {
			self.fRec55[l147 as usize] = 0.0;
		}
		for l148 in 0..16384 {
			self.fVec65[l148 as usize] = 0.0;
		}
		for l149 in 0..2 {
			self.fVec66[l149 as usize] = 0.0;
		}
		for l150 in 0..2 {
			self.fRec56[l150 as usize] = 0.0;
		}
		for l151 in 0..16384 {
			self.fVec67[l151 as usize] = 0.0;
		}
		for l152 in 0..2 {
			self.fRec89[l152 as usize] = 0.0;
		}
		for l153 in 0..2 {
			self.fVec68[l153 as usize] = 0.0;
		}
		for l154 in 0..2 {
			self.fRec57[l154 as usize] = 0.0;
		}
		for l155 in 0..16384 {
			self.fVec69[l155 as usize] = 0.0;
		}
		for l156 in 0..2 {
			self.fVec70[l156 as usize] = 0.0;
		}
		for l157 in 0..2 {
			self.fRec58[l157 as usize] = 0.0;
		}
		for l158 in 0..16384 {
			self.fVec71[l158 as usize] = 0.0;
		}
		for l159 in 0..2 {
			self.fRec90[l159 as usize] = 0.0;
		}
		for l160 in 0..2 {
			self.fVec72[l160 as usize] = 0.0;
		}
		for l161 in 0..2 {
			self.fRec59[l161 as usize] = 0.0;
		}
		for l162 in 0..2 {
			self.fRec60[l162 as usize] = 0.0;
		}
		for l163 in 0..2 {
			self.fRec61[l163 as usize] = 0.0;
		}
	}
	pub fn instance_constants(&mut self, sample_rate: i32) {
		// Obtaining locks on 0 static var(s)
		self.fSampleRate = sample_rate;
		self.fConst0 = F32::min(1.92e+05, F32::max(1.0, (self.fSampleRate) as F32));
		self.fConst1 = 2.0833333e-05 * self.fConst0;
		self.fConst2 = 12.566371 / self.fConst0;
		self.fConst3 = F32::sin(self.fConst2);
		self.fConst4 = F32::cos(self.fConst2);
		self.fConst5 = F32::tan(6283.1855 / self.fConst0);
		self.fConst6 = 1.0 / self.fConst5;
		self.fConst7 = 1.0 / (self.fConst6 + 1.0);
		self.fConst8 = 1.0 - self.fConst6;
		self.fConst9 = (self.fConst6 + 1.618034) / self.fConst5 + 1.0;
		self.fConst10 = 1.0 / self.fConst9;
		self.fConst11 = (self.fConst6 + -1.618034) / self.fConst5 + 1.0;
		self.fConst12 = Dsp_faustpower2_f(self.fConst5);
		self.fConst13 = 2.0 * (1.0 - 1.0 / self.fConst12);
		self.fConst14 = (self.fConst6 + 0.618034) / self.fConst5 + 1.0;
		self.fConst15 = 1.0 / self.fConst14;
		self.fConst16 = (self.fConst6 + -0.618034) / self.fConst5 + 1.0;
		self.fConst17 = 1.0 / (self.fConst12 * self.fConst9);
		self.fConst18 = 1.0 / (self.fConst12 * self.fConst14);
		self.fConst19 = F32::tan(1570.7964 / self.fConst0);
		self.fConst20 = 1.0 / self.fConst19;
		self.fConst21 = 1.0 / ((self.fConst20 + 1.618034) / self.fConst19 + 1.0);
		self.fConst22 = (self.fConst20 + -1.618034) / self.fConst19 + 1.0;
		self.fConst23 = Dsp_faustpower2_f(self.fConst19);
		self.fConst24 = 1.0 / self.fConst23;
		self.fConst25 = 2.0 * (1.0 - self.fConst24);
		self.fConst26 = 1.0 / (self.fConst20 + 1.0);
		self.fConst27 = 1.0 - self.fConst20;
		self.fConst28 = 1.0 / (self.fConst19 * self.fConst14);
		self.fConst29 = (self.fConst20 + 1.618034) / self.fConst19 + 1.0;
		self.fConst30 = 1.0 / self.fConst29;
		self.fConst31 = (self.fConst20 + -1.618034) / self.fConst19 + 1.0;
		self.fConst32 = 1.0 / ((self.fConst20 + 0.618034) / self.fConst19 + 1.0);
		self.fConst33 = (self.fConst20 + -0.618034) / self.fConst19 + 1.0;
		self.fConst34 = 1.0 / (self.fConst23 * self.fConst29);
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
		ui_interface.open_vertical_box("jp");
		ui_interface.add_horizontal_slider("damp", ParamIndex(0), 0.3, 0.0, 1.0, 0.001);
		ui_interface.add_horizontal_slider("decay", ParamIndex(1), 2.0, 0.1, 2e+01, 0.01);
		ui_interface.add_horizontal_slider("size", ParamIndex(2), 1.0, 0.5, 5.0, 0.001);
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
	self.fSlow0 = F32::min(5.0, self.fConst1 * (self.fHslider0) as F32);
		self.iSlow1 = self.itbl0DspSIG0[((1e+01 * self.fSlow0) as i32) as usize];
		self.fSlow2 = 0.0001 * (self.iSlow1) as F32;
		self.iSlow3 = self.itbl0DspSIG0[((1.1e+02 * self.fSlow0) as i32) as usize];
		self.fSlow4 = 0.0001 * (self.iSlow3) as F32;
		self.iSlow5 = self.itbl0DspSIG0[((1.4e+02 * self.fSlow0) as i32) as usize];
		self.fSlow6 = 0.0001 * (self.iSlow5) as F32;
		self.iSlow7 = self.itbl0DspSIG0[((4e+01 * self.fSlow0) as i32) as usize];
		self.fSlow8 = 0.0001 * (self.iSlow7) as F32;
		self.iSlow9 = self.itbl0DspSIG0[((1.7e+02 * self.fSlow0) as i32) as usize];
		self.fSlow10 = 0.0001 * (self.iSlow9) as F32;
		self.iSlow11 = self.itbl0DspSIG0[((7e+01 * self.fSlow0) as i32) as usize];
		self.fSlow12 = 0.0001 * (self.iSlow11) as F32;
		self.iSlow13 = self.itbl0DspSIG0[((1e+02 * self.fSlow0) as i32) as usize];
		self.fSlow14 = 0.0001 * (self.iSlow13) as F32;
		self.iSlow15 = self.itbl0DspSIG0[((2e+02 * self.fSlow0) as i32) as usize];
		self.fSlow16 = 0.0001 * (self.iSlow15) as F32;
		self.iSlow17 = self.itbl0DspSIG0[((2.3e+02 * self.fSlow0) as i32) as usize];
		self.fSlow18 = 0.0001 * (self.iSlow17) as F32;
		self.iSlow19 = self.itbl0DspSIG0[((204.0 * self.fSlow0) as i32) as usize];
		self.fSlow20 = 0.005 * (self.iSlow19) as F32;
		self.iSlow21 = self.itbl0DspSIG0[((1.3e+02 * self.fSlow0) as i32) as usize];
		self.fSlow22 = 0.0001 * (self.iSlow21) as F32;
		self.iSlow23 = self.itbl0DspSIG0[((54.0 * self.fSlow0) as i32) as usize];
		self.fSlow24 = 0.005 * (self.iSlow23) as F32;
		self.iSlow25 = self.itbl0DspSIG0[((125.0 * self.fSlow0) as i32) as usize];
		self.fSlow26 = 0.0001 * (self.iSlow25) as F32;
		self.iSlow27 = self.itbl0DspSIG0[((25.0 * self.fSlow0) as i32) as usize];
		self.fSlow28 = 0.0001 * (self.iSlow27) as F32;
		self.iSlow29 = self.itbl0DspSIG0[((55.0 * self.fSlow0) as i32) as usize];
		self.fSlow30 = 0.0001 * (self.iSlow29) as F32;
		self.iSlow31 = self.itbl0DspSIG0[((155.0 * self.fSlow0) as i32) as usize];
		self.fSlow32 = 0.0001 * (self.iSlow31) as F32;
		self.iSlow33 = self.itbl0DspSIG0[((85.0 * self.fSlow0) as i32) as usize];
		self.fSlow34 = 0.0001 * (self.iSlow33) as F32;
		self.iSlow35 = self.itbl0DspSIG0[((185.0 * self.fSlow0) as i32) as usize];
		self.fSlow36 = 0.0001 * (self.iSlow35) as F32;
		self.iSlow37 = self.itbl0DspSIG0[((115.0 * self.fSlow0) as i32) as usize];
		self.fSlow38 = 0.0001 * (self.iSlow37) as F32;
		self.iSlow39 = self.itbl0DspSIG0[((215.0 * self.fSlow0) as i32) as usize];
		self.fSlow40 = 0.0001 * (self.iSlow39) as F32;
		self.iSlow41 = self.itbl0DspSIG0[((245.0 * self.fSlow0) as i32) as usize];
		self.fSlow42 = 0.0001 * (self.iSlow41) as F32;
		self.iSlow43 = self.itbl0DspSIG0[((145.0 * self.fSlow0) as i32) as usize];
		self.fSlow44 = 0.0001 * (self.iSlow43) as F32;
		self.iSlow45 = self.itbl0DspSIG0[((134.0 * self.fSlow0) as i32) as usize];
		self.fSlow46 = 0.005 * (self.iSlow45) as F32;
		self.iSlow47 = self.itbl0DspSIG0[((34.0 * self.fSlow0) as i32) as usize];
		self.fSlow48 = 0.005 * (self.iSlow47) as F32;
		self.fSlow49 = (self.fHslider1) as F32;
		self.fSlow50 = 1.0 - self.fSlow49;
		self.fSlow51 = F32::powf(1e+01, -(0.51 * ((1.25 * self.fSlow0 + -0.25) / (self.fHslider2) as F32)));
		self.iSlow52 = self.itbl0DspSIG0[((2.4e+02 * self.fSlow0) as i32) as usize];
		self.fSlow53 = 0.0001 * (self.iSlow52) as F32;
		self.iSlow54 = self.itbl0DspSIG0[((1.9e+02 * self.fSlow0) as i32) as usize];
		self.fSlow55 = 0.0001 * (self.iSlow54) as F32;
		self.iSlow56 = self.itbl0DspSIG0[((175.0 * self.fSlow0) as i32) as usize];
		self.fSlow57 = 0.0001 * (self.iSlow56) as F32;
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
		let mut fTemp0: F32 = 0.7602446 * self.fRec61[1] - 0.6496369 * self.fRec0[1];
		let mut fTemp1: F32 = 0.6496369 * self.fRec1[1];
		let mut fTemp2: F32 = 0.7602446 * self.fRec60[1];
		self.fVec1[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp0 + 0.70710677 * (fTemp1 - fTemp2);
		let mut iTemp3: i32 = i32::wrapping_sub(1, self.iVec0[1]);
		let mut fTemp4: F32 = self.fSlow2 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow1)) as F32 + self.fRec62[1]);
		self.fRec62[0] = (if (F32::abs(fTemp4) > 1.1754944e-38) as i32 != 0 {fTemp4} else {0.0});
		let mut fTemp5: F32 = -1.49999 + self.fRec62[0];
		let mut fTemp6: F32 = self.fVec1[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp5) as i32)))) & 16383) as usize];
		self.fVec2[0] = fTemp6;
		let mut fTemp7: F32 = F32::floor(fTemp5);
		let mut fTemp8: F32 = self.fVec2[1] - (2.0 - self.fRec62[0] + fTemp7) * (self.fRec0[1] - fTemp6) / (self.fRec62[0] - fTemp7);
		self.fRec0[0] = (if (F32::abs(fTemp8) > 1.1754944e-38) as i32 != 0 {fTemp8} else {0.0});
		self.fVec3[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp0 + 0.70710677 * (fTemp2 - fTemp1);
		let mut fTemp9: F32 = self.fSlow4 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow3)) as F32 + self.fRec63[1]);
		self.fRec63[0] = (if (F32::abs(fTemp9) > 1.1754944e-38) as i32 != 0 {fTemp9} else {0.0});
		let mut fTemp10: F32 = -1.49999 + self.fRec63[0];
		let mut fTemp11: F32 = self.fVec3[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp10) as i32)))) & 16383) as usize];
		self.fVec4[0] = fTemp11;
		let mut fTemp12: F32 = F32::floor(fTemp10);
		let mut fTemp13: F32 = self.fVec4[1] - (2.0 - self.fRec63[0] + fTemp12) * (self.fRec1[1] - fTemp11) / (self.fRec63[0] - fTemp12);
		self.fRec1[0] = (if (F32::abs(fTemp13) > 1.1754944e-38) as i32 != 0 {fTemp13} else {0.0});
		let mut fTemp14: F32 = 0.7602446 * self.fRec0[1] + 0.6496369 * self.fRec61[1];
		let mut fTemp15: F32 = 0.7602446 * fTemp14 - 0.6496369 * self.fRec3[1];
		let mut fTemp16: F32 = 0.7602446 * self.fRec1[1] + 0.6496369 * self.fRec60[1];
		let mut fTemp17: F32 = 0.7602446 * fTemp16;
		let mut fTemp18: F32 = 0.6496369 * self.fRec2[1];
		self.fVec5[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp15 + 0.70710677 * (fTemp17 - fTemp18);
		let mut fTemp19: F32 = self.fSlow6 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow5)) as F32 + self.fRec64[1]);
		self.fRec64[0] = (if (F32::abs(fTemp19) > 1.1754944e-38) as i32 != 0 {fTemp19} else {0.0});
		let mut fTemp20: F32 = -1.49999 + self.fRec64[0];
		let mut fTemp21: F32 = self.fVec5[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp20) as i32)))) & 16383) as usize];
		self.fVec6[0] = fTemp21;
		let mut fTemp22: F32 = F32::floor(fTemp20);
		let mut fTemp23: F32 = self.fVec6[1] - (2.0 - self.fRec64[0] + fTemp22) * (self.fRec2[1] - fTemp21) / (self.fRec64[0] - fTemp22);
		self.fRec2[0] = (if (F32::abs(fTemp23) > 1.1754944e-38) as i32 != 0 {fTemp23} else {0.0});
		self.fVec7[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp15 + 0.70710677 * (fTemp18 - fTemp17);
		let mut fTemp24: F32 = self.fSlow8 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow7)) as F32 + self.fRec65[1]);
		self.fRec65[0] = (if (F32::abs(fTemp24) > 1.1754944e-38) as i32 != 0 {fTemp24} else {0.0});
		let mut fTemp25: F32 = -1.49999 + self.fRec65[0];
		let mut fTemp26: F32 = self.fVec7[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp25) as i32)))) & 16383) as usize];
		self.fVec8[0] = fTemp26;
		let mut fTemp27: F32 = F32::floor(fTemp25);
		let mut fTemp28: F32 = self.fVec8[1] - (2.0 - self.fRec65[0] + fTemp27) * (self.fRec3[1] - fTemp26) / (self.fRec65[0] - fTemp27);
		self.fRec3[0] = (if (F32::abs(fTemp28) > 1.1754944e-38) as i32 != 0 {fTemp28} else {0.0});
		let mut fTemp29: F32 = 0.7602446 * self.fRec3[1] + 0.6496369 * fTemp14;
		let mut fTemp30: F32 = 0.7602446 * fTemp29 - 0.6496369 * self.fRec5[1];
		let mut fTemp31: F32 = 0.7602446 * self.fRec2[1] + 0.6496369 * fTemp16;
		let mut fTemp32: F32 = 0.7602446 * fTemp31;
		let mut fTemp33: F32 = 0.6496369 * self.fRec4[1];
		self.fVec9[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp30 + 0.70710677 * (fTemp32 - fTemp33);
		let mut fTemp34: F32 = self.fSlow10 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow9)) as F32 + self.fRec66[1]);
		self.fRec66[0] = (if (F32::abs(fTemp34) > 1.1754944e-38) as i32 != 0 {fTemp34} else {0.0});
		let mut fTemp35: F32 = -1.49999 + self.fRec66[0];
		let mut fTemp36: F32 = self.fVec9[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp35) as i32)))) & 16383) as usize];
		self.fVec10[0] = fTemp36;
		let mut fTemp37: F32 = F32::floor(fTemp35);
		let mut fTemp38: F32 = self.fVec10[1] - (2.0 - self.fRec66[0] + fTemp37) * (self.fRec4[1] - fTemp36) / (self.fRec66[0] - fTemp37);
		self.fRec4[0] = (if (F32::abs(fTemp38) > 1.1754944e-38) as i32 != 0 {fTemp38} else {0.0});
		self.fVec11[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp30 + 0.70710677 * (fTemp33 - fTemp32);
		let mut fTemp39: F32 = self.fSlow12 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow11)) as F32 + self.fRec67[1]);
		self.fRec67[0] = (if (F32::abs(fTemp39) > 1.1754944e-38) as i32 != 0 {fTemp39} else {0.0});
		let mut fTemp40: F32 = -1.49999 + self.fRec67[0];
		let mut fTemp41: F32 = self.fVec11[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp40) as i32)))) & 16383) as usize];
		self.fVec12[0] = fTemp41;
		let mut fTemp42: F32 = F32::floor(fTemp40);
		let mut fTemp43: F32 = self.fVec12[1] - (2.0 - self.fRec67[0] + fTemp42) * (self.fRec5[1] - fTemp41) / (self.fRec67[0] - fTemp42);
		self.fRec5[0] = (if (F32::abs(fTemp43) > 1.1754944e-38) as i32 != 0 {fTemp43} else {0.0});
		let mut fTemp44: F32 = 0.7602446 * self.fRec5[1] + 0.6496369 * fTemp29;
		let mut fTemp45: F32 = 0.7602446 * fTemp44 - 0.6496369 * self.fRec6[1];
		let mut fTemp46: F32 = 0.6496369 * self.fRec7[1];
		let mut fTemp47: F32 = 0.7602446 * self.fRec4[1] + 0.6496369 * fTemp31;
		let mut fTemp48: F32 = 0.7602446 * fTemp47;
		self.fVec13[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp45 + 0.70710677 * (fTemp46 - fTemp48);
		let mut fTemp49: F32 = self.fSlow14 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow13)) as F32 + self.fRec68[1]);
		self.fRec68[0] = (if (F32::abs(fTemp49) > 1.1754944e-38) as i32 != 0 {fTemp49} else {0.0});
		let mut fTemp50: F32 = -1.49999 + self.fRec68[0];
		let mut fTemp51: F32 = self.fVec13[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp50) as i32)))) & 16383) as usize];
		self.fVec14[0] = fTemp51;
		let mut fTemp52: F32 = F32::floor(fTemp50);
		let mut fTemp53: F32 = self.fVec14[1] - (2.0 - self.fRec68[0] + fTemp52) * (self.fRec6[1] - fTemp51) / (self.fRec68[0] - fTemp52);
		self.fRec6[0] = (if (F32::abs(fTemp53) > 1.1754944e-38) as i32 != 0 {fTemp53} else {0.0});
		self.fVec15[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp45 + 0.70710677 * (fTemp48 - fTemp46);
		let mut fTemp54: F32 = self.fSlow16 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow15)) as F32 + self.fRec69[1]);
		self.fRec69[0] = (if (F32::abs(fTemp54) > 1.1754944e-38) as i32 != 0 {fTemp54} else {0.0});
		let mut fTemp55: F32 = -1.49999 + self.fRec69[0];
		let mut fTemp56: F32 = self.fVec15[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp55) as i32)))) & 16383) as usize];
		self.fVec16[0] = fTemp56;
		let mut fTemp57: F32 = F32::floor(fTemp55);
		let mut fTemp58: F32 = self.fVec16[1] - (2.0 - self.fRec69[0] + fTemp57) * (self.fRec7[1] - fTemp56) / (self.fRec69[0] - fTemp57);
		self.fRec7[0] = (if (F32::abs(fTemp58) > 1.1754944e-38) as i32 != 0 {fTemp58} else {0.0});
		let mut fTemp59: F32 = 0.7602446 * self.fRec6[1] + 0.6496369 * fTemp44;
		let mut fTemp60: F32 = 0.7602446 * fTemp59 - 0.6496369 * self.fRec10[1];
		let mut fTemp61: F32 = 0.7602446 * self.fRec7[1] + 0.6496369 * fTemp47;
		let mut fTemp62: F32 = 0.7602446 * fTemp61;
		let mut fTemp63: F32 = 0.6496369 * self.fRec8[1];
		self.fVec17[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp60 + 0.70710677 * (fTemp62 - fTemp63);
		let mut fTemp64: F32 = self.fSlow18 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow17)) as F32 + self.fRec70[1]);
		self.fRec70[0] = (if (F32::abs(fTemp64) > 1.1754944e-38) as i32 != 0 {fTemp64} else {0.0});
		let mut fTemp65: F32 = -1.49999 + self.fRec70[0];
		let mut fTemp66: F32 = self.fVec17[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp65) as i32)))) & 16383) as usize];
		self.fVec18[0] = fTemp66;
		let mut fTemp67: F32 = F32::floor(fTemp65);
		let mut fTemp68: F32 = self.fVec18[1] - (2.0 - self.fRec70[0] + fTemp67) * (self.fRec8[1] - fTemp66) / (self.fRec70[0] - fTemp67);
		self.fRec8[0] = (if (F32::abs(fTemp68) > 1.1754944e-38) as i32 != 0 {fTemp68} else {0.0});
		let mut fTemp69: F32 = (iTemp3) as F32 - self.fConst3 * self.fRec72[1] + self.fConst4 * self.fRec71[1];
		self.fRec71[0] = (if (F32::abs(fTemp69) > 1.1754944e-38) as i32 != 0 {fTemp69} else {0.0});
		let mut fTemp70: F32 = self.fConst3 * self.fRec71[1] + self.fConst4 * self.fRec72[1];
		self.fRec72[0] = (if (F32::abs(fTemp70) > 1.1754944e-38) as i32 != 0 {fTemp70} else {0.0});
		let mut fTemp71: F32 = 5.0 * (1.0 + (1.0 - self.fRec71[0]));
		let mut fTemp72: F32 = -1.499995 + fTemp71;
		let mut fTemp73: F32 = F32::floor(fTemp72);
		let mut fTemp74: F32 = -3.0 - fTemp73 + fTemp71;
		let mut fTemp75: F32 = -2.0 - fTemp73 + fTemp71;
		let mut fTemp76: F32 = 0.7602446 * self.fRec8[1] + 0.6496369 * fTemp61;
		self.fVec19[(self.IOTA0 & 1023) as usize] = fTemp76;
		let mut iTemp77: i32 = (fTemp72) as i32;
		let mut fTemp78: F32 = -1.0 - fTemp73 + fTemp71;
		let mut fTemp79: F32 = fTemp71 - fTemp73;
		let mut fTemp80: F32 = fTemp79 * fTemp78;
		let mut fTemp81: F32 = fTemp80 * fTemp75;
		self.fVec20[(self.IOTA0 & 16383) as usize] = (-4.0 - fTemp73 + fTemp71) * (fTemp74 * (fTemp75 * (0.041666668 * self.fVec19[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, core::cmp::max(0, iTemp77)))) & 1023) as usize] * fTemp78 - 0.16666667 * fTemp79 * self.fVec19[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(1, iTemp77))))) & 1023) as usize]) + 0.25 * fTemp80 * self.fVec19[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(2, iTemp77))))) & 1023) as usize]) - 0.16666667 * fTemp81 * self.fVec19[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(3, iTemp77))))) & 1023) as usize]) + 0.041666668 * fTemp81 * fTemp74 * self.fVec19[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(4, iTemp77))))) & 1023) as usize];
		let mut fTemp82: F32 = self.fSlow20 + 0.995 * ((i32::wrapping_mul(iTemp3, self.iSlow19)) as F32 + self.fRec73[1]);
		self.fRec73[0] = (if (F32::abs(fTemp82) > 1.1754944e-38) as i32 != 0 {fTemp82} else {0.0});
		let mut fTemp83: F32 = -1.49999 + self.fRec73[0];
		let mut fTemp84: F32 = self.fVec20[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp83) as i32)))) & 16383) as usize];
		self.fVec21[0] = fTemp84;
		let mut fTemp85: F32 = F32::floor(fTemp83);
		let mut fTemp86: F32 = self.fVec21[1] - (2.0 - self.fRec73[0] + fTemp85) * (self.fRec9[1] - fTemp84) / (self.fRec73[0] - fTemp85);
		self.fRec9[0] = (if (F32::abs(fTemp86) > 1.1754944e-38) as i32 != 0 {fTemp86} else {0.0});
		self.fVec22[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp60 + 0.70710677 * (fTemp63 - fTemp62);
		let mut fTemp87: F32 = self.fSlow22 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow21)) as F32 + self.fRec74[1]);
		self.fRec74[0] = (if (F32::abs(fTemp87) > 1.1754944e-38) as i32 != 0 {fTemp87} else {0.0});
		let mut fTemp88: F32 = -1.49999 + self.fRec74[0];
		let mut fTemp89: F32 = self.fVec22[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp88) as i32)))) & 16383) as usize];
		self.fVec23[0] = fTemp89;
		let mut fTemp90: F32 = F32::floor(fTemp88);
		let mut fTemp91: F32 = self.fVec23[1] - (2.0 - self.fRec74[0] + fTemp90) * (self.fRec10[1] - fTemp89) / (self.fRec74[0] - fTemp90);
		self.fRec10[0] = (if (F32::abs(fTemp91) > 1.1754944e-38) as i32 != 0 {fTemp91} else {0.0});
		let mut fTemp92: F32 = 5.0 * (1.0 + 1.0 + self.fRec71[0]);
		let mut fTemp93: F32 = -1.499995 + fTemp92;
		let mut fTemp94: F32 = F32::floor(fTemp93);
		let mut fTemp95: F32 = -4.0 - fTemp94 + fTemp92;
		let mut fTemp96: F32 = -3.0 - fTemp94 + fTemp92;
		let mut fTemp97: F32 = -2.0 - fTemp94 + fTemp92;
		let mut fTemp98: F32 = 0.7602446 * self.fRec10[1] + 0.6496369 * fTemp59;
		self.fVec24[(self.IOTA0 & 1023) as usize] = fTemp98;
		let mut iTemp99: i32 = (fTemp93) as i32;
		let mut iTemp100: i32 = core::cmp::min(512, core::cmp::max(0, iTemp99));
		let mut fTemp101: F32 = -1.0 - fTemp94 + fTemp92;
		let mut fTemp102: F32 = fTemp92 - fTemp94;
		let mut iTemp103: i32 = core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(1, iTemp99)));
		let mut fTemp104: F32 = fTemp102 * fTemp101;
		let mut iTemp105: i32 = core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(2, iTemp99)));
		let mut fTemp106: F32 = fTemp104 * fTemp97;
		let mut iTemp107: i32 = core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(3, iTemp99)));
		let mut fTemp108: F32 = fTemp106 * fTemp96;
		let mut iTemp109: i32 = core::cmp::min(512, core::cmp::max(0, i32::wrapping_add(4, iTemp99)));
		self.fVec25[(self.IOTA0 & 16383) as usize] = fTemp95 * (fTemp96 * (fTemp97 * (0.041666668 * self.fVec24[((i32::wrapping_sub(self.IOTA0, iTemp100)) & 1023) as usize] * fTemp101 - 0.16666667 * fTemp102 * self.fVec24[((i32::wrapping_sub(self.IOTA0, iTemp103)) & 1023) as usize]) + 0.25 * fTemp104 * self.fVec24[((i32::wrapping_sub(self.IOTA0, iTemp105)) & 1023) as usize]) - 0.16666667 * fTemp106 * self.fVec24[((i32::wrapping_sub(self.IOTA0, iTemp107)) & 1023) as usize]) + 0.041666668 * fTemp108 * self.fVec24[((i32::wrapping_sub(self.IOTA0, iTemp109)) & 1023) as usize];
		let mut fTemp110: F32 = self.fSlow24 + 0.995 * ((i32::wrapping_mul(iTemp3, self.iSlow23)) as F32 + self.fRec75[1]);
		self.fRec75[0] = (if (F32::abs(fTemp110) > 1.1754944e-38) as i32 != 0 {fTemp110} else {0.0});
		let mut fTemp111: F32 = -1.49999 + self.fRec75[0];
		let mut fTemp112: F32 = self.fVec25[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp111) as i32)))) & 16383) as usize];
		self.fVec26[0] = fTemp112;
		let mut fTemp113: F32 = F32::floor(fTemp111);
		let mut fTemp114: F32 = self.fVec26[1] - (2.0 - self.fRec75[0] + fTemp113) * (self.fRec11[1] - fTemp112) / (self.fRec75[0] - fTemp113);
		self.fRec11[0] = (if (F32::abs(fTemp114) > 1.1754944e-38) as i32 != 0 {fTemp114} else {0.0});
		let mut fTemp115: F32 = 0.7602446 * self.fRec11[0] - 0.6496369 * self.fRec12[1];
		let mut fTemp116: F32 = 0.6496369 * self.fRec13[1];
		let mut fTemp117: F32 = 0.7602446 * self.fRec9[0];
		self.fVec27[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp115 + 0.70710677 * (fTemp116 - fTemp117);
		let mut fTemp118: F32 = self.fSlow26 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow25)) as F32 + self.fRec76[1]);
		self.fRec76[0] = (if (F32::abs(fTemp118) > 1.1754944e-38) as i32 != 0 {fTemp118} else {0.0});
		let mut fTemp119: F32 = -1.49999 + self.fRec76[0];
		let mut fTemp120: F32 = self.fVec27[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp119) as i32)))) & 16383) as usize];
		self.fVec28[0] = fTemp120;
		let mut fTemp121: F32 = F32::floor(fTemp119);
		let mut fTemp122: F32 = self.fVec28[1] - (2.0 - self.fRec76[0] + fTemp121) * (self.fRec12[1] - fTemp120) / (self.fRec76[0] - fTemp121);
		self.fRec12[0] = (if (F32::abs(fTemp122) > 1.1754944e-38) as i32 != 0 {fTemp122} else {0.0});
		self.fVec29[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp115 + 0.70710677 * (fTemp117 - fTemp116);
		let mut fTemp123: F32 = self.fSlow28 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow27)) as F32 + self.fRec77[1]);
		self.fRec77[0] = (if (F32::abs(fTemp123) > 1.1754944e-38) as i32 != 0 {fTemp123} else {0.0});
		let mut fTemp124: F32 = -1.49999 + self.fRec77[0];
		let mut fTemp125: F32 = self.fVec29[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp124) as i32)))) & 16383) as usize];
		self.fVec30[0] = fTemp125;
		let mut fTemp126: F32 = F32::floor(fTemp124);
		let mut fTemp127: F32 = self.fVec30[1] - (2.0 - self.fRec77[0] + fTemp126) * (self.fRec13[1] - fTemp125) / (self.fRec77[0] - fTemp126);
		self.fRec13[0] = (if (F32::abs(fTemp127) > 1.1754944e-38) as i32 != 0 {fTemp127} else {0.0});
		let mut fTemp128: F32 = 0.7602446 * self.fRec12[1] + 0.6496369 * self.fRec11[0];
		let mut fTemp129: F32 = 0.7602446 * fTemp128 - 0.6496369 * self.fRec15[1];
		let mut fTemp130: F32 = 0.7602446 * self.fRec13[1] + 0.6496369 * self.fRec9[0];
		let mut fTemp131: F32 = 0.7602446 * fTemp130;
		let mut fTemp132: F32 = 0.6496369 * self.fRec14[1];
		self.fVec31[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp129 + 0.70710677 * (fTemp131 - fTemp132);
		let mut fTemp133: F32 = self.fSlow30 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow29)) as F32 + self.fRec78[1]);
		self.fRec78[0] = (if (F32::abs(fTemp133) > 1.1754944e-38) as i32 != 0 {fTemp133} else {0.0});
		let mut fTemp134: F32 = -1.49999 + self.fRec78[0];
		let mut iTemp135: i32 = core::cmp::min(8192, core::cmp::max(0, (fTemp134) as i32));
		let mut fTemp136: F32 = self.fVec31[((i32::wrapping_sub(self.IOTA0, iTemp135)) & 16383) as usize];
		self.fVec32[0] = fTemp136;
		let mut fTemp137: F32 = F32::floor(fTemp134);
		let mut fTemp138: F32 = 2.0 - self.fRec78[0] + fTemp137;
		let mut fTemp139: F32 = self.fRec78[0] - fTemp137;
		let mut fTemp140: F32 = self.fVec32[1] - fTemp138 * (self.fRec14[1] - fTemp136) / fTemp139;
		self.fRec14[0] = (if (F32::abs(fTemp140) > 1.1754944e-38) as i32 != 0 {fTemp140} else {0.0});
		self.fVec33[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp129 + 0.70710677 * (fTemp132 - fTemp131);
		let mut fTemp141: F32 = self.fSlow32 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow31)) as F32 + self.fRec79[1]);
		self.fRec79[0] = (if (F32::abs(fTemp141) > 1.1754944e-38) as i32 != 0 {fTemp141} else {0.0});
		let mut fTemp142: F32 = -1.49999 + self.fRec79[0];
		let mut fTemp143: F32 = self.fVec33[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp142) as i32)))) & 16383) as usize];
		self.fVec34[0] = fTemp143;
		let mut fTemp144: F32 = F32::floor(fTemp142);
		let mut fTemp145: F32 = self.fVec34[1] - (2.0 - self.fRec79[0] + fTemp144) * (self.fRec15[1] - fTemp143) / (self.fRec79[0] - fTemp144);
		self.fRec15[0] = (if (F32::abs(fTemp145) > 1.1754944e-38) as i32 != 0 {fTemp145} else {0.0});
		let mut fTemp146: F32 = 0.7602446 * self.fRec15[1] + 0.6496369 * fTemp128;
		let mut fTemp147: F32 = 0.7602446 * fTemp146 - 0.6496369 * self.fRec17[1];
		let mut fTemp148: F32 = 0.7602446 * self.fRec14[1] + 0.6496369 * fTemp130;
		let mut fTemp149: F32 = 0.7602446 * fTemp148;
		let mut fTemp150: F32 = 0.6496369 * self.fRec16[1];
		self.fVec35[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp147 + 0.70710677 * (fTemp149 - fTemp150);
		let mut fTemp151: F32 = self.fSlow34 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow33)) as F32 + self.fRec80[1]);
		self.fRec80[0] = (if (F32::abs(fTemp151) > 1.1754944e-38) as i32 != 0 {fTemp151} else {0.0});
		let mut fTemp152: F32 = -1.49999 + self.fRec80[0];
		let mut iTemp153: i32 = core::cmp::min(8192, core::cmp::max(0, (fTemp152) as i32));
		let mut fTemp154: F32 = self.fVec35[((i32::wrapping_sub(self.IOTA0, iTemp153)) & 16383) as usize];
		self.fVec36[0] = fTemp154;
		let mut fTemp155: F32 = F32::floor(fTemp152);
		let mut fTemp156: F32 = 2.0 - self.fRec80[0] + fTemp155;
		let mut fTemp157: F32 = self.fRec80[0] - fTemp155;
		let mut fTemp158: F32 = self.fVec36[1] - fTemp156 * (self.fRec16[1] - fTemp154) / fTemp157;
		self.fRec16[0] = (if (F32::abs(fTemp158) > 1.1754944e-38) as i32 != 0 {fTemp158} else {0.0});
		self.fVec37[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp147 + 0.70710677 * (fTemp150 - fTemp149);
		let mut fTemp159: F32 = self.fSlow36 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow35)) as F32 + self.fRec81[1]);
		self.fRec81[0] = (if (F32::abs(fTemp159) > 1.1754944e-38) as i32 != 0 {fTemp159} else {0.0});
		let mut fTemp160: F32 = -1.49999 + self.fRec81[0];
		let mut fTemp161: F32 = self.fVec37[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp160) as i32)))) & 16383) as usize];
		self.fVec38[0] = fTemp161;
		let mut fTemp162: F32 = F32::floor(fTemp160);
		let mut fTemp163: F32 = self.fVec38[1] - (2.0 - self.fRec81[0] + fTemp162) * (self.fRec17[1] - fTemp161) / (self.fRec81[0] - fTemp162);
		self.fRec17[0] = (if (F32::abs(fTemp163) > 1.1754944e-38) as i32 != 0 {fTemp163} else {0.0});
		let mut fTemp164: F32 = 0.7602446 * self.fRec17[1] + 0.6496369 * fTemp146;
		let mut fTemp165: F32 = 0.7602446 * fTemp164 - 0.6496369 * self.fRec19[1];
		let mut fTemp166: F32 = 0.7602446 * self.fRec16[1] + 0.6496369 * fTemp148;
		let mut fTemp167: F32 = 0.7602446 * fTemp166;
		let mut fTemp168: F32 = 0.6496369 * self.fRec18[1];
		self.fVec39[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp165 + 0.70710677 * (fTemp167 - fTemp168);
		let mut fTemp169: F32 = self.fSlow38 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow37)) as F32 + self.fRec82[1]);
		self.fRec82[0] = (if (F32::abs(fTemp169) > 1.1754944e-38) as i32 != 0 {fTemp169} else {0.0});
		let mut fTemp170: F32 = -1.49999 + self.fRec82[0];
		let mut iTemp171: i32 = core::cmp::min(8192, core::cmp::max(0, (fTemp170) as i32));
		let mut fTemp172: F32 = self.fVec39[((i32::wrapping_sub(self.IOTA0, iTemp171)) & 16383) as usize];
		self.fVec40[0] = fTemp172;
		let mut fTemp173: F32 = F32::floor(fTemp170);
		let mut fTemp174: F32 = 2.0 - self.fRec82[0] + fTemp173;
		let mut fTemp175: F32 = self.fRec82[0] - fTemp173;
		let mut fTemp176: F32 = self.fVec40[1] - fTemp174 * (self.fRec18[1] - fTemp172) / fTemp175;
		self.fRec18[0] = (if (F32::abs(fTemp176) > 1.1754944e-38) as i32 != 0 {fTemp176} else {0.0});
		self.fVec41[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp165 + 0.70710677 * (fTemp168 - fTemp167);
		let mut fTemp177: F32 = self.fSlow40 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow39)) as F32 + self.fRec83[1]);
		self.fRec83[0] = (if (F32::abs(fTemp177) > 1.1754944e-38) as i32 != 0 {fTemp177} else {0.0});
		let mut fTemp178: F32 = -1.49999 + self.fRec83[0];
		let mut iTemp179: i32 = core::cmp::min(8192, core::cmp::max(0, (fTemp178) as i32));
		let mut fTemp180: F32 = self.fVec41[((i32::wrapping_sub(self.IOTA0, iTemp179)) & 16383) as usize];
		self.fVec42[0] = fTemp180;
		let mut fTemp181: F32 = F32::floor(fTemp178);
		let mut fTemp182: F32 = 2.0 - self.fRec83[0] + fTemp181;
		let mut fTemp183: F32 = self.fRec83[0] - fTemp181;
		let mut fTemp184: F32 = self.fVec42[1] - fTemp182 * (self.fRec19[1] - fTemp180) / fTemp183;
		self.fRec19[0] = (if (F32::abs(fTemp184) > 1.1754944e-38) as i32 != 0 {fTemp184} else {0.0});
		let mut fTemp185: F32 = 0.7602446 * self.fRec19[1] + 0.6496369 * fTemp164;
		let mut fTemp186: F32 = 0.7602446 * fTemp185 - 0.6496369 * self.fRec20[1];
		let mut fTemp187: F32 = 0.6496369 * self.fRec21[1];
		let mut fTemp188: F32 = 0.7602446 * self.fRec18[1] + 0.6496369 * fTemp166;
		let mut fTemp189: F32 = 0.7602446 * fTemp188;
		self.fVec43[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp186 + 0.70710677 * (fTemp187 - fTemp189);
		let mut fTemp190: F32 = self.fSlow42 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow41)) as F32 + self.fRec84[1]);
		self.fRec84[0] = (if (F32::abs(fTemp190) > 1.1754944e-38) as i32 != 0 {fTemp190} else {0.0});
		let mut fTemp191: F32 = -1.49999 + self.fRec84[0];
		let mut fTemp192: F32 = self.fVec43[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp191) as i32)))) & 16383) as usize];
		self.fVec44[0] = fTemp192;
		let mut fTemp193: F32 = F32::floor(fTemp191);
		let mut fTemp194: F32 = self.fVec44[1] - (2.0 - self.fRec84[0] + fTemp193) * (self.fRec20[1] - fTemp192) / (self.fRec84[0] - fTemp193);
		self.fRec20[0] = (if (F32::abs(fTemp194) > 1.1754944e-38) as i32 != 0 {fTemp194} else {0.0});
		self.fVec45[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp186 + 0.70710677 * (fTemp189 - fTemp187);
		let mut fTemp195: F32 = self.fSlow44 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow43)) as F32 + self.fRec85[1]);
		self.fRec85[0] = (if (F32::abs(fTemp195) > 1.1754944e-38) as i32 != 0 {fTemp195} else {0.0});
		let mut fTemp196: F32 = -1.49999 + self.fRec85[0];
		let mut iTemp197: i32 = core::cmp::min(8192, core::cmp::max(0, (fTemp196) as i32));
		let mut fTemp198: F32 = self.fVec45[((i32::wrapping_sub(self.IOTA0, iTemp197)) & 16383) as usize];
		self.fVec46[0] = fTemp198;
		let mut fTemp199: F32 = F32::floor(fTemp196);
		let mut fTemp200: F32 = 2.0 - self.fRec85[0] + fTemp199;
		let mut fTemp201: F32 = self.fRec85[0] - fTemp199;
		let mut fTemp202: F32 = self.fVec46[1] - fTemp200 * (self.fRec21[1] - fTemp198) / fTemp201;
		self.fRec21[0] = (if (F32::abs(fTemp202) > 1.1754944e-38) as i32 != 0 {fTemp202} else {0.0});
		let mut fTemp203: F32 = 5.0 * (1.0 + 1.0 + self.fRec72[0]);
		let mut fTemp204: F32 = -1.499995 + fTemp203;
		let mut fTemp205: F32 = F32::floor(fTemp204);
		let mut fTemp206: F32 = -4.0 - fTemp205 + fTemp203;
		let mut fTemp207: F32 = -3.0 - fTemp205 + fTemp203;
		let mut fTemp208: F32 = -2.0 - fTemp205 + fTemp203;
		let mut fTemp209: F32 = 0.7602446 * self.fRec20[1] + 0.6496369 * fTemp185;
		self.fVec47[(self.IOTA0 & 16383) as usize] = fTemp209;
		let mut iTemp210: i32 = (fTemp204) as i32;
		let mut iTemp211: i32 = core::cmp::max(0, iTemp210);
		let mut fTemp212: F32 = -1.0 - fTemp205 + fTemp203;
		let mut fTemp213: F32 = fTemp203 - fTemp205;
		let mut iTemp214: i32 = core::cmp::max(0, i32::wrapping_add(1, iTemp210));
		let mut fTemp215: F32 = fTemp213 * fTemp212;
		let mut iTemp216: i32 = core::cmp::max(0, i32::wrapping_add(2, iTemp210));
		let mut fTemp217: F32 = fTemp215 * fTemp208;
		let mut iTemp218: i32 = core::cmp::max(0, i32::wrapping_add(3, iTemp210));
		let mut fTemp219: F32 = fTemp217 * fTemp207;
		let mut iTemp220: i32 = core::cmp::max(0, i32::wrapping_add(4, iTemp210));
		self.fVec48[(self.IOTA0 & 16383) as usize] = fTemp206 * (fTemp207 * (fTemp208 * (0.041666668 * self.fVec47[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, iTemp211))) & 16383) as usize] * fTemp212 - 0.16666667 * fTemp213 * self.fVec47[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, iTemp214))) & 16383) as usize]) + 0.25 * fTemp215 * self.fVec47[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, iTemp216))) & 16383) as usize]) - 0.16666667 * fTemp217 * self.fVec47[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, iTemp218))) & 16383) as usize]) + 0.041666668 * fTemp219 * self.fVec47[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, iTemp220))) & 16383) as usize];
		let mut fTemp221: F32 = self.fSlow46 + 0.995 * ((i32::wrapping_mul(iTemp3, self.iSlow45)) as F32 + self.fRec86[1]);
		self.fRec86[0] = (if (F32::abs(fTemp221) > 1.1754944e-38) as i32 != 0 {fTemp221} else {0.0});
		let mut fTemp222: F32 = -1.49999 + self.fRec86[0];
		let mut fTemp223: F32 = self.fVec48[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp222) as i32)))) & 16383) as usize];
		self.fVec49[0] = fTemp223;
		let mut fTemp224: F32 = F32::floor(fTemp222);
		let mut fTemp225: F32 = self.fVec49[1] - (2.0 - self.fRec86[0] + fTemp224) * (self.fRec22[1] - fTemp223) / (self.fRec86[0] - fTemp224);
		self.fRec22[0] = (if (F32::abs(fTemp225) > 1.1754944e-38) as i32 != 0 {fTemp225} else {0.0});
		let mut fTemp226: F32 = 5.0 * (1.0 + (1.0 - self.fRec72[0]));
		let mut fTemp227: F32 = -1.499995 + fTemp226;
		let mut fTemp228: F32 = F32::floor(fTemp227);
		let mut fTemp229: F32 = -3.0 - fTemp228 + fTemp226;
		let mut fTemp230: F32 = -2.0 - fTemp228 + fTemp226;
		let mut fTemp231: F32 = 0.7602446 * self.fRec21[1] + 0.6496369 * fTemp188;
		self.fVec50[(self.IOTA0 & 16383) as usize] = fTemp231;
		let mut iTemp232: i32 = (fTemp227) as i32;
		let mut fTemp233: F32 = -1.0 - fTemp228 + fTemp226;
		let mut fTemp234: F32 = fTemp226 - fTemp228;
		let mut fTemp235: F32 = fTemp234 * fTemp233;
		let mut fTemp236: F32 = fTemp235 * fTemp230;
		self.fVec51[(self.IOTA0 & 16383) as usize] = (-4.0 - fTemp228 + fTemp226) * (fTemp229 * (fTemp230 * (0.041666668 * self.fVec50[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, iTemp232)))) & 16383) as usize] * fTemp233 - 0.16666667 * fTemp234 * self.fVec50[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, i32::wrapping_add(1, iTemp232))))) & 16383) as usize]) + 0.25 * fTemp235 * self.fVec50[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, i32::wrapping_add(2, iTemp232))))) & 16383) as usize]) - 0.16666667 * fTemp236 * self.fVec50[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, i32::wrapping_add(3, iTemp232))))) & 16383) as usize]) + 0.041666668 * fTemp236 * fTemp229 * self.fVec50[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, i32::wrapping_add(4, iTemp232))))) & 16383) as usize];
		let mut fTemp237: F32 = self.fSlow48 + 0.995 * ((i32::wrapping_mul(iTemp3, self.iSlow47)) as F32 + self.fRec87[1]);
		self.fRec87[0] = (if (F32::abs(fTemp237) > 1.1754944e-38) as i32 != 0 {fTemp237} else {0.0});
		let mut fTemp238: F32 = -1.49999 + self.fRec87[0];
		let mut fTemp239: F32 = self.fVec51[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp238) as i32)))) & 16383) as usize];
		self.fVec52[0] = fTemp239;
		let mut fTemp240: F32 = F32::floor(fTemp238);
		let mut fTemp241: F32 = self.fVec52[1] - (2.0 - self.fRec87[0] + fTemp240) * (self.fRec23[1] - fTemp239) / (self.fRec87[0] - fTemp240);
		self.fRec23[0] = (if (F32::abs(fTemp241) > 1.1754944e-38) as i32 != 0 {fTemp241} else {0.0});
		let mut fTemp242: F32 = self.fConst7 * (self.fConst8 * self.fRec24[1] - self.fConst6 * (self.fRec23[0] - self.fRec23[1]));
		self.fRec24[0] = (if (F32::abs(-fTemp242) > 1.1754944e-38) as i32 != 0 {-fTemp242} else {0.0});
		let mut fTemp243: F32 = self.fConst7 * (self.fConst8 * self.fRec25[1] - (self.fRec23[1] + self.fRec23[0]));
		self.fRec25[0] = (if (F32::abs(-fTemp243) > 1.1754944e-38) as i32 != 0 {-fTemp243} else {0.0});
		let mut fTemp244: F32 = self.fConst7 * (self.fConst8 * self.fRec26[1] - (self.fRec22[1] + self.fRec22[0]));
		self.fRec26[0] = (if (F32::abs(-fTemp244) > 1.1754944e-38) as i32 != 0 {-fTemp244} else {0.0});
		let mut fTemp245: F32 = self.fConst7 * (self.fConst8 * self.fRec27[1] - self.fConst6 * (self.fRec22[0] - self.fRec22[1]));
		self.fRec27[0] = (if (F32::abs(-fTemp245) > 1.1754944e-38) as i32 != 0 {-fTemp245} else {0.0});
		let mut fTemp246: F32 = self.fRec25[0] - self.fConst10 * (self.fConst11 * self.fRec28[2] + self.fConst13 * self.fRec28[1]);
		self.fRec28[0] = (if (F32::abs(fTemp246) > 1.1754944e-38) as i32 != 0 {fTemp246} else {0.0});
		let mut fTemp247: F32 = self.fRec26[0] - self.fConst10 * (self.fConst11 * self.fRec29[2] + self.fConst13 * self.fRec29[1]);
		self.fRec29[0] = (if (F32::abs(fTemp247) > 1.1754944e-38) as i32 != 0 {fTemp247} else {0.0});
		let mut fTemp248: F32 = self.fRec24[0] - self.fConst10 * (self.fConst11 * self.fRec30[2] + self.fConst13 * self.fRec30[1]);
		self.fRec30[0] = (if (F32::abs(fTemp248) > 1.1754944e-38) as i32 != 0 {fTemp248} else {0.0});
		let mut fTemp249: F32 = self.fRec27[0] - self.fConst10 * (self.fConst11 * self.fRec31[2] + self.fConst13 * self.fRec31[1]);
		self.fRec31[0] = (if (F32::abs(fTemp249) > 1.1754944e-38) as i32 != 0 {fTemp249} else {0.0});
		let mut fTemp250: F32 = self.fConst10 * (self.fRec29[2] + self.fRec29[0] + 2.0 * self.fRec29[1]) - self.fConst15 * (self.fConst16 * self.fRec32[2] + self.fConst13 * self.fRec32[1]);
		self.fRec32[0] = (if (F32::abs(fTemp250) > 1.1754944e-38) as i32 != 0 {fTemp250} else {0.0});
		let mut fTemp251: F32 = self.fConst17 * (self.fRec30[2] + (self.fRec30[0] - 2.0 * self.fRec30[1])) - self.fConst15 * (self.fConst16 * self.fRec33[2] + self.fConst13 * self.fRec33[1]);
		self.fRec33[0] = (if (F32::abs(fTemp251) > 1.1754944e-38) as i32 != 0 {fTemp251} else {0.0});
		let mut fTemp252: F32 = self.fConst17 * (self.fRec31[2] + (self.fRec31[0] - 2.0 * self.fRec31[1])) - self.fConst15 * (self.fConst16 * self.fRec34[2] + self.fConst13 * self.fRec34[1]);
		self.fRec34[0] = (if (F32::abs(fTemp252) > 1.1754944e-38) as i32 != 0 {fTemp252} else {0.0});
		let mut fTemp253: F32 = self.fConst25 * self.fRec35[1];
		let mut fTemp254: F32 = self.fConst18 * (self.fRec34[2] + (self.fRec34[0] - 2.0 * self.fRec34[1])) - self.fConst21 * (self.fConst22 * self.fRec35[2] + fTemp253);
		self.fRec35[0] = (if (F32::abs(fTemp254) > 1.1754944e-38) as i32 != 0 {fTemp254} else {0.0});
		let mut fTemp255: F32 = self.fConst10 * (self.fRec28[2] + self.fRec28[0] + 2.0 * self.fRec28[1]) - self.fConst15 * (self.fConst16 * self.fRec36[2] + self.fConst13 * self.fRec36[1]);
		self.fRec36[0] = (if (F32::abs(fTemp255) > 1.1754944e-38) as i32 != 0 {fTemp255} else {0.0});
		let mut fTemp256: F32 = self.fRec32[2] + self.fRec32[0] + 2.0 * self.fRec32[1];
		self.fVec53[0] = fTemp256;
		let mut fTemp257: F32 = self.fConst26 * (self.fConst27 * self.fRec37[1] - self.fConst15 * (fTemp256 + self.fVec53[1]));
		self.fRec37[0] = (if (F32::abs(-fTemp257) > 1.1754944e-38) as i32 != 0 {-fTemp257} else {0.0});
		let mut fTemp258: F32 = self.fConst26 * (self.fConst27 * self.fRec38[1] - self.fConst28 * (fTemp256 - self.fVec53[1]));
		self.fRec38[0] = (if (F32::abs(-fTemp258) > 1.1754944e-38) as i32 != 0 {-fTemp258} else {0.0});
		let mut fTemp259: F32 = self.fConst25 * self.fRec39[1];
		let mut fTemp260: F32 = self.fConst18 * (self.fRec33[2] + (self.fRec33[0] - 2.0 * self.fRec33[1])) - self.fConst21 * (self.fConst22 * self.fRec39[2] + fTemp259);
		self.fRec39[0] = (if (F32::abs(fTemp260) > 1.1754944e-38) as i32 != 0 {fTemp260} else {0.0});
		let mut fTemp261: F32 = self.fRec38[0] - self.fConst30 * (self.fConst31 * self.fRec40[2] + self.fConst25 * self.fRec40[1]);
		self.fRec40[0] = (if (F32::abs(fTemp261) > 1.1754944e-38) as i32 != 0 {fTemp261} else {0.0});
		let mut fTemp262: F32 = self.fRec37[0] - self.fConst30 * (self.fConst31 * self.fRec41[2] + self.fConst25 * self.fRec41[1]);
		self.fRec41[0] = (if (F32::abs(fTemp262) > 1.1754944e-38) as i32 != 0 {fTemp262} else {0.0});
		let mut fTemp263: F32 = self.fRec36[2] + self.fRec36[0] + 2.0 * self.fRec36[1];
		self.fVec54[0] = fTemp263;
		let mut fTemp264: F32 = self.fConst26 * (self.fConst27 * self.fRec42[1] - self.fConst28 * (fTemp263 - self.fVec54[1]));
		self.fRec42[0] = (if (F32::abs(-fTemp264) > 1.1754944e-38) as i32 != 0 {-fTemp264} else {0.0});
		let mut fTemp265: F32 = self.fConst26 * (self.fConst27 * self.fRec43[1] - self.fConst15 * (fTemp263 + self.fVec54[1]));
		self.fRec43[0] = (if (F32::abs(-fTemp265) > 1.1754944e-38) as i32 != 0 {-fTemp265} else {0.0});
		let mut fTemp266: F32 = self.fConst30 * (self.fRec41[2] + self.fRec41[0] + 2.0 * self.fRec41[1]) - self.fConst32 * (self.fConst33 * self.fRec44[2] + self.fConst25 * self.fRec44[1]);
		self.fRec44[0] = (if (F32::abs(fTemp266) > 1.1754944e-38) as i32 != 0 {fTemp266} else {0.0});
		let mut fTemp267: F32 = self.fConst34 * (self.fRec40[2] + (self.fRec40[0] - 2.0 * self.fRec40[1])) - self.fConst32 * (self.fConst33 * self.fRec45[2] + self.fConst25 * self.fRec45[1]);
		self.fRec45[0] = (if (F32::abs(fTemp267) > 1.1754944e-38) as i32 != 0 {fTemp267} else {0.0});
		let mut fTemp268: F32 = self.fRec43[0] - self.fConst30 * (self.fConst31 * self.fRec46[2] + self.fConst25 * self.fRec46[1]);
		self.fRec46[0] = (if (F32::abs(fTemp268) > 1.1754944e-38) as i32 != 0 {fTemp268} else {0.0});
		let mut fTemp269: F32 = self.fRec42[0] - self.fConst30 * (self.fConst31 * self.fRec47[2] + self.fConst25 * self.fRec47[1]);
		self.fRec47[0] = (if (F32::abs(fTemp269) > 1.1754944e-38) as i32 != 0 {fTemp269} else {0.0});
		let mut fTemp270: F32 = self.fConst34 * (self.fRec47[2] + (self.fRec47[0] - 2.0 * self.fRec47[1])) - self.fConst32 * (self.fConst33 * self.fRec48[2] + self.fConst25 * self.fRec48[1]);
		self.fRec48[0] = (if (F32::abs(fTemp270) > 1.1754944e-38) as i32 != 0 {fTemp270} else {0.0});
		let mut fTemp271: F32 = self.fConst30 * (self.fRec46[2] + self.fRec46[0] + 2.0 * self.fRec46[1]) - self.fConst32 * (self.fConst33 * self.fRec49[2] + self.fConst25 * self.fRec49[1]);
		self.fRec49[0] = (if (F32::abs(fTemp271) > 1.1754944e-38) as i32 != 0 {fTemp271} else {0.0});
		let mut fTemp272: F32 = (*input0) as F32 + self.fSlow51 * (self.fRec35[2] + self.fConst21 * (fTemp253 + self.fConst22 * self.fRec35[0]) + self.fConst32 * (self.fRec44[2] + self.fRec44[0] + 2.0 * self.fRec44[1] + self.fConst24 * (self.fRec45[2] + (self.fRec45[0] - 2.0 * self.fRec45[1]))));
		self.fVec55[(self.IOTA0 & 1023) as usize] = fTemp272;
		let mut fTemp273: F32 = self.fSlow50 * (fTemp206 * (fTemp207 * (fTemp208 * (0.041666668 * fTemp212 * self.fVec55[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, iTemp211))) & 1023) as usize] - 0.16666667 * fTemp213 * self.fVec55[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, iTemp214))) & 1023) as usize]) + 0.25 * fTemp215 * self.fVec55[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, iTemp216))) & 1023) as usize]) - 0.16666667 * fTemp217 * self.fVec55[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, iTemp218))) & 1023) as usize]) + 0.041666668 * fTemp219 * self.fVec55[((i32::wrapping_sub(self.IOTA0, core::cmp::min(512, iTemp220))) & 1023) as usize]) + self.fSlow49 * self.fRec50[1];
		self.fRec50[0] = (if (F32::abs(fTemp273) > 1.1754944e-38) as i32 != 0 {fTemp273} else {0.0});
		let mut fTemp274: F32 = (*input1) as F32 + self.fSlow51 * (self.fRec39[2] + self.fConst21 * (fTemp259 + self.fConst22 * self.fRec39[0]) + self.fConst32 * (self.fRec49[2] + self.fRec49[0] + 2.0 * self.fRec49[1] + self.fConst24 * (self.fRec48[2] + (self.fRec48[0] - 2.0 * self.fRec48[1]))));
		self.fVec56[(self.IOTA0 & 1023) as usize] = fTemp274;
		let mut fTemp275: F32 = self.fSlow50 * (fTemp95 * (fTemp96 * (fTemp97 * (0.041666668 * fTemp101 * self.fVec56[((i32::wrapping_sub(self.IOTA0, iTemp100)) & 1023) as usize] - 0.16666667 * fTemp102 * self.fVec56[((i32::wrapping_sub(self.IOTA0, iTemp103)) & 1023) as usize]) + 0.25 * fTemp104 * self.fVec56[((i32::wrapping_sub(self.IOTA0, iTemp105)) & 1023) as usize]) - 0.16666667 * fTemp106 * self.fVec56[((i32::wrapping_sub(self.IOTA0, iTemp107)) & 1023) as usize]) + 0.041666668 * fTemp108 * self.fVec56[((i32::wrapping_sub(self.IOTA0, iTemp109)) & 1023) as usize]) + self.fSlow49 * self.fRec51[1];
		self.fRec51[0] = (if (F32::abs(fTemp275) > 1.1754944e-38) as i32 != 0 {fTemp275} else {0.0});
		let mut fTemp276: F32 = 0.760314 * self.fRec50[0] - 0.64955574 * self.fRec52[1];
		let mut fTemp277: F32 = 0.64955574 * self.fRec53[1];
		let mut fTemp278: F32 = 0.760314 * self.fRec51[0];
		self.fVec57[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp276 + 0.70710677 * (fTemp277 - fTemp278);
		let mut fTemp279: F32 = self.fVec57[((i32::wrapping_sub(self.IOTA0, iTemp135)) & 16383) as usize];
		self.fVec58[0] = fTemp279;
		let mut fTemp280: F32 = self.fVec58[1] - fTemp138 * (self.fRec52[1] - fTemp279) / fTemp139;
		self.fRec52[0] = (if (F32::abs(fTemp280) > 1.1754944e-38) as i32 != 0 {fTemp280} else {0.0});
		self.fVec59[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp276 + 0.70710677 * (fTemp278 - fTemp277);
		let mut fTemp281: F32 = self.fSlow53 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow52)) as F32 + self.fRec88[1]);
		self.fRec88[0] = (if (F32::abs(fTemp281) > 1.1754944e-38) as i32 != 0 {fTemp281} else {0.0});
		let mut fTemp282: F32 = -1.49999 + self.fRec88[0];
		let mut fTemp283: F32 = self.fVec59[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp282) as i32)))) & 16383) as usize];
		self.fVec60[0] = fTemp283;
		let mut fTemp284: F32 = F32::floor(fTemp282);
		let mut fTemp285: F32 = self.fVec60[1] - (2.0 - self.fRec88[0] + fTemp284) * (self.fRec53[1] - fTemp283) / (self.fRec88[0] - fTemp284);
		self.fRec53[0] = (if (F32::abs(fTemp285) > 1.1754944e-38) as i32 != 0 {fTemp285} else {0.0});
		let mut fTemp286: F32 = 0.760314 * self.fRec52[1] + 0.64955574 * self.fRec50[0];
		let mut fTemp287: F32 = 0.760314 * fTemp286 - 0.64955574 * self.fRec55[1];
		let mut fTemp288: F32 = 0.760314 * self.fRec53[1] + 0.64955574 * self.fRec51[0];
		let mut fTemp289: F32 = 0.760314 * fTemp288;
		let mut fTemp290: F32 = 0.64955574 * self.fRec54[1];
		self.fVec61[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp287 + 0.70710677 * (fTemp289 - fTemp290);
		let mut fTemp291: F32 = self.fVec61[((i32::wrapping_sub(self.IOTA0, iTemp153)) & 16383) as usize];
		self.fVec62[0] = fTemp291;
		let mut fTemp292: F32 = self.fVec62[1] - fTemp156 * (self.fRec54[1] - fTemp291) / fTemp157;
		self.fRec54[0] = (if (F32::abs(fTemp292) > 1.1754944e-38) as i32 != 0 {fTemp292} else {0.0});
		self.fVec63[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp287 + 0.70710677 * (fTemp290 - fTemp289);
		let mut fTemp293: F32 = self.fVec63[((i32::wrapping_sub(self.IOTA0, iTemp179)) & 16383) as usize];
		self.fVec64[0] = fTemp293;
		let mut fTemp294: F32 = self.fVec64[1] - fTemp182 * (self.fRec55[1] - fTemp293) / fTemp183;
		self.fRec55[0] = (if (F32::abs(fTemp294) > 1.1754944e-38) as i32 != 0 {fTemp294} else {0.0});
		let mut fTemp295: F32 = 0.760314 * self.fRec55[1] + 0.64955574 * fTemp286;
		let mut fTemp296: F32 = 0.760314 * fTemp295 - 0.64955574 * self.fRec56[1];
		let mut fTemp297: F32 = 0.64955574 * self.fRec57[1];
		let mut fTemp298: F32 = 0.760314 * self.fRec54[1] + 0.64955574 * fTemp288;
		let mut fTemp299: F32 = 0.760314 * fTemp298;
		self.fVec65[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp296 + 0.70710677 * (fTemp297 - fTemp299);
		let mut fTemp300: F32 = self.fVec65[((i32::wrapping_sub(self.IOTA0, iTemp171)) & 16383) as usize];
		self.fVec66[0] = fTemp300;
		let mut fTemp301: F32 = self.fVec66[1] - fTemp174 * (self.fRec56[1] - fTemp300) / fTemp175;
		self.fRec56[0] = (if (F32::abs(fTemp301) > 1.1754944e-38) as i32 != 0 {fTemp301} else {0.0});
		self.fVec67[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp296 + 0.70710677 * (fTemp299 - fTemp297);
		let mut fTemp302: F32 = self.fSlow55 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow54)) as F32 + self.fRec89[1]);
		self.fRec89[0] = (if (F32::abs(fTemp302) > 1.1754944e-38) as i32 != 0 {fTemp302} else {0.0});
		let mut fTemp303: F32 = -1.49999 + self.fRec89[0];
		let mut fTemp304: F32 = self.fVec67[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp303) as i32)))) & 16383) as usize];
		self.fVec68[0] = fTemp304;
		let mut fTemp305: F32 = F32::floor(fTemp303);
		let mut fTemp306: F32 = self.fVec68[1] - (2.0 - self.fRec89[0] + fTemp305) * (self.fRec57[1] - fTemp304) / (self.fRec89[0] - fTemp305);
		self.fRec57[0] = (if (F32::abs(fTemp306) > 1.1754944e-38) as i32 != 0 {fTemp306} else {0.0});
		let mut fTemp307: F32 = 0.760314 * self.fRec56[1] + 0.64955574 * fTemp295;
		let mut fTemp308: F32 = 0.760314 * fTemp307 - 0.64955574 * self.fRec59[1];
		let mut fTemp309: F32 = 0.760314 * self.fRec57[1] + 0.64955574 * fTemp298;
		let mut fTemp310: F32 = 0.760314 * fTemp309;
		let mut fTemp311: F32 = 0.64955574 * self.fRec58[1];
		self.fVec69[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp308 + 0.70710677 * (fTemp310 - fTemp311);
		let mut fTemp312: F32 = self.fVec69[((i32::wrapping_sub(self.IOTA0, iTemp197)) & 16383) as usize];
		self.fVec70[0] = fTemp312;
		let mut fTemp313: F32 = self.fVec70[1] - fTemp200 * (self.fRec58[1] - fTemp312) / fTemp201;
		self.fRec58[0] = (if (F32::abs(fTemp313) > 1.1754944e-38) as i32 != 0 {fTemp313} else {0.0});
		self.fVec71[(self.IOTA0 & 16383) as usize] = 0.70710677 * fTemp308 + 0.70710677 * (fTemp311 - fTemp310);
		let mut fTemp314: F32 = self.fSlow57 + 0.9999 * ((i32::wrapping_mul(iTemp3, self.iSlow56)) as F32 + self.fRec90[1]);
		self.fRec90[0] = (if (F32::abs(fTemp314) > 1.1754944e-38) as i32 != 0 {fTemp314} else {0.0});
		let mut fTemp315: F32 = -1.49999 + self.fRec90[0];
		let mut fTemp316: F32 = self.fVec71[((i32::wrapping_sub(self.IOTA0, core::cmp::min(8192, core::cmp::max(0, (fTemp315) as i32)))) & 16383) as usize];
		self.fVec72[0] = fTemp316;
		let mut fTemp317: F32 = F32::floor(fTemp315);
		let mut fTemp318: F32 = self.fVec72[1] - (2.0 - self.fRec90[0] + fTemp317) * (self.fRec59[1] - fTemp316) / (self.fRec90[0] - fTemp317);
		self.fRec59[0] = (if (F32::abs(fTemp318) > 1.1754944e-38) as i32 != 0 {fTemp318} else {0.0});
		let mut fTemp319: F32 = 0.760314 * self.fRec58[1] + 0.64955574 * fTemp309;
		self.fRec60[0] = (if (F32::abs(fTemp319) > 1.1754944e-38) as i32 != 0 {fTemp319} else {0.0});
		let mut fTemp320: F32 = 0.760314 * self.fRec59[1] + 0.64955574 * fTemp307;
		self.fRec61[0] = (if (F32::abs(fTemp320) > 1.1754944e-38) as i32 != 0 {fTemp320} else {0.0});
		*output0 = (self.fRec61[0]) as FaustFloat;
		*output1 = (self.fRec60[0]) as FaustFloat;
		self.iVec0[1] = self.iVec0[0];
		self.IOTA0 = i32::wrapping_add(self.IOTA0, 1);
		self.fRec62[1] = self.fRec62[0];
		self.fVec2[1] = self.fVec2[0];
		self.fRec0[1] = self.fRec0[0];
		self.fRec63[1] = self.fRec63[0];
		self.fVec4[1] = self.fVec4[0];
		self.fRec1[1] = self.fRec1[0];
		self.fRec64[1] = self.fRec64[0];
		self.fVec6[1] = self.fVec6[0];
		self.fRec2[1] = self.fRec2[0];
		self.fRec65[1] = self.fRec65[0];
		self.fVec8[1] = self.fVec8[0];
		self.fRec3[1] = self.fRec3[0];
		self.fRec66[1] = self.fRec66[0];
		self.fVec10[1] = self.fVec10[0];
		self.fRec4[1] = self.fRec4[0];
		self.fRec67[1] = self.fRec67[0];
		self.fVec12[1] = self.fVec12[0];
		self.fRec5[1] = self.fRec5[0];
		self.fRec68[1] = self.fRec68[0];
		self.fVec14[1] = self.fVec14[0];
		self.fRec6[1] = self.fRec6[0];
		self.fRec69[1] = self.fRec69[0];
		self.fVec16[1] = self.fVec16[0];
		self.fRec7[1] = self.fRec7[0];
		self.fRec70[1] = self.fRec70[0];
		self.fVec18[1] = self.fVec18[0];
		self.fRec8[1] = self.fRec8[0];
		self.fRec71[1] = self.fRec71[0];
		self.fRec72[1] = self.fRec72[0];
		self.fRec73[1] = self.fRec73[0];
		self.fVec21[1] = self.fVec21[0];
		self.fRec9[1] = self.fRec9[0];
		self.fRec74[1] = self.fRec74[0];
		self.fVec23[1] = self.fVec23[0];
		self.fRec10[1] = self.fRec10[0];
		self.fRec75[1] = self.fRec75[0];
		self.fVec26[1] = self.fVec26[0];
		self.fRec11[1] = self.fRec11[0];
		self.fRec76[1] = self.fRec76[0];
		self.fVec28[1] = self.fVec28[0];
		self.fRec12[1] = self.fRec12[0];
		self.fRec77[1] = self.fRec77[0];
		self.fVec30[1] = self.fVec30[0];
		self.fRec13[1] = self.fRec13[0];
		self.fRec78[1] = self.fRec78[0];
		self.fVec32[1] = self.fVec32[0];
		self.fRec14[1] = self.fRec14[0];
		self.fRec79[1] = self.fRec79[0];
		self.fVec34[1] = self.fVec34[0];
		self.fRec15[1] = self.fRec15[0];
		self.fRec80[1] = self.fRec80[0];
		self.fVec36[1] = self.fVec36[0];
		self.fRec16[1] = self.fRec16[0];
		self.fRec81[1] = self.fRec81[0];
		self.fVec38[1] = self.fVec38[0];
		self.fRec17[1] = self.fRec17[0];
		self.fRec82[1] = self.fRec82[0];
		self.fVec40[1] = self.fVec40[0];
		self.fRec18[1] = self.fRec18[0];
		self.fRec83[1] = self.fRec83[0];
		self.fVec42[1] = self.fVec42[0];
		self.fRec19[1] = self.fRec19[0];
		self.fRec84[1] = self.fRec84[0];
		self.fVec44[1] = self.fVec44[0];
		self.fRec20[1] = self.fRec20[0];
		self.fRec85[1] = self.fRec85[0];
		self.fVec46[1] = self.fVec46[0];
		self.fRec21[1] = self.fRec21[0];
		self.fRec86[1] = self.fRec86[0];
		self.fVec49[1] = self.fVec49[0];
		self.fRec22[1] = self.fRec22[0];
		self.fRec87[1] = self.fRec87[0];
		self.fVec52[1] = self.fVec52[0];
		self.fRec23[1] = self.fRec23[0];
		self.fRec24[1] = self.fRec24[0];
		self.fRec25[1] = self.fRec25[0];
		self.fRec26[1] = self.fRec26[0];
		self.fRec27[1] = self.fRec27[0];
		self.fRec28[2] = self.fRec28[1];
		self.fRec28[1] = self.fRec28[0];
		self.fRec29[2] = self.fRec29[1];
		self.fRec29[1] = self.fRec29[0];
		self.fRec30[2] = self.fRec30[1];
		self.fRec30[1] = self.fRec30[0];
		self.fRec31[2] = self.fRec31[1];
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
		self.fVec53[1] = self.fVec53[0];
		self.fRec37[1] = self.fRec37[0];
		self.fRec38[1] = self.fRec38[0];
		self.fRec39[2] = self.fRec39[1];
		self.fRec39[1] = self.fRec39[0];
		self.fRec40[2] = self.fRec40[1];
		self.fRec40[1] = self.fRec40[0];
		self.fRec41[2] = self.fRec41[1];
		self.fRec41[1] = self.fRec41[0];
		self.fVec54[1] = self.fVec54[0];
		self.fRec42[1] = self.fRec42[0];
		self.fRec43[1] = self.fRec43[0];
		self.fRec44[2] = self.fRec44[1];
		self.fRec44[1] = self.fRec44[0];
		self.fRec45[2] = self.fRec45[1];
		self.fRec45[1] = self.fRec45[0];
		self.fRec46[2] = self.fRec46[1];
		self.fRec46[1] = self.fRec46[0];
		self.fRec47[2] = self.fRec47[1];
		self.fRec47[1] = self.fRec47[0];
		self.fRec48[2] = self.fRec48[1];
		self.fRec48[1] = self.fRec48[0];
		self.fRec49[2] = self.fRec49[1];
		self.fRec49[1] = self.fRec49[0];
		self.fRec50[1] = self.fRec50[0];
		self.fRec51[1] = self.fRec51[0];
		self.fVec58[1] = self.fVec58[0];
		self.fRec52[1] = self.fRec52[0];
		self.fRec88[1] = self.fRec88[0];
		self.fVec60[1] = self.fVec60[0];
		self.fRec53[1] = self.fRec53[0];
		self.fVec62[1] = self.fVec62[0];
		self.fRec54[1] = self.fRec54[0];
		self.fVec64[1] = self.fVec64[0];
		self.fRec55[1] = self.fRec55[0];
		self.fVec66[1] = self.fVec66[0];
		self.fRec56[1] = self.fRec56[0];
		self.fRec89[1] = self.fRec89[0];
		self.fVec68[1] = self.fVec68[0];
		self.fRec57[1] = self.fRec57[0];
		self.fVec70[1] = self.fVec70[0];
		self.fRec58[1] = self.fRec58[0];
		self.fRec90[1] = self.fRec90[0];
		self.fVec72[1] = self.fVec72[0];
		self.fRec59[1] = self.fRec59[0];
		self.fRec60[1] = self.fRec60[0];
		self.fRec61[1] = self.fRec61[0];
		
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

