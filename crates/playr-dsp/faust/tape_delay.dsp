// Tape echo: the read position drifts with wow and flutter, and a soft clip
// in the loop saturates the repeats. Wet only.
declare name "tape_delay";
declare license "MIT";
import("stdfaust.lib");

// si.smoo, but at its target on the first frame after a clear, so a
// cleared delay's first echo is on time. After reverbs.lib's smooth_init.
smoo(x) = x : *(1 - p) : + ~ (+(x * first) : *(p))
with {
    p = ba.tau2pole(0.02);
    first = 1 - 1';
};

MAX = 384000;
time = hslider("time", 0.375, 0.001, 2, 0.0001) : smoo;
feedback = hslider("feedback", 0.4, 0, 0.95, 0.001) : smoo;
tone = hslider("tone", 6000, 500, 20000, 1) : smoo;
wow = hslider("wow", 0.3, 0, 1, 0.001) : smoo;

// Peak pitch deviation 2 pi f D: 0.5% at 0.6 Hz, 0.1% at 9 Hz, with slow
// noise. Depths in seconds, centred on the time.
drift(p) = wow * (0.0013 * os.oscp(0.6, p) + 0.0000177 * os.oscp(9, p)
    + 0.0003 * (no.noise : fi.lowpass(2, 3)));
// Unity gain below 2/3, 1 at 4/3 and above.
clip = *(0.5) : aa.softclipQuadratic1;

// As in pingpong.dsp: the line a frame short, the output a frame late.
channel(p) = (+ : de.fdelay(MAX, max(0, (time + drift(p)) * ma.SR - 1)))
    ~ (fi.lowpass(1, tone) : *(feedback) : clip) : mem;
process = channel(0), channel(ma.PI / 2);
