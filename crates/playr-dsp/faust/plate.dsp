// Dattorro's plate. Its lines are fixed sample counts, tuned for 29.8 kHz:
// the plate is smaller at higher rates.
declare name "plate";
import("stdfaust.lib");

decay = hslider("decay", 2, 0.1, 20, 0.01);
damp = hslider("damp", 0.3, 0, 1, 0.001);
// The tank applies decay 4 times a loop of 21,589 samples, once every
// 5,397; 5,800 is that calibrated to the measured tail. g^(t60/pass) = 10^-3.
pass = 5800 / ma.SR;
g = min(0.9999, pow(10, -3 * pass / decay));
process = re.dattorro_rev(0, 0.9995, 0.75, 0.625, g, 0.7, 0.5, damp * 0.9);
