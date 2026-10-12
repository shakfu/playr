// jpverb (Julian Parker): a modulated hall. Its lines are prime sample
// counts set by size, so size is scaled by the rate to keep the room.
declare name "jp";
import("stdfaust.lib");

decay = hslider("decay", 2, 0.1, 20, 0.01);
damp = hslider("damp", 0.3, 0, 1, 0.001);
size = hslider("size", 1, 0.5, 5, 0.001);
// Its lines cap at 8192 samples, which size 5 nearly fills at 48 kHz.
scaled = min(5, size * ma.SR / 48000);
process = re.jpverb(decay, damp, scaled, 0.707, 0.1, 2, 1, 1, 1, 500, 2000);
