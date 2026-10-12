// Zita-rev1: an 8-line FDN with separate low and mid decay. Wet only.
declare name "zita";
import("stdfaust.lib");

decay = hslider("decay", 2, 0.1, 20, 0.01);
// Damping lowers the frequency above which the tail decays at half the rate.
damp = hslider("damp", 0.3, 0, 1, 0.001);
f2 = 20000 * pow(1500 / 20000, damp);
process = re.zita_rev1_stereo(0, 200, f2, decay, decay, 192000);
