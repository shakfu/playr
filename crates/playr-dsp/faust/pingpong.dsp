// Ping-pong delay: each repeat crosses to the other channel. Wet only.
declare name "pingpong";
declare license "MIT";
import("stdfaust.lib");

// si.smoo, but at its target on the first frame after a clear, so a
// cleared delay's first echo is on time. After reverbs.lib's smooth_init.
smoo(x) = x : *(1 - p) : + ~ (+(x * first) : *(p))
with {
    p = ba.tau2pole(0.02);
    first = 1 - 1';
};

// 2 s at 192 kHz.
MAX = 384000;
time = hslider("time", 0.375, 0.001, 2, 0.0001) : smoo;
feedback = hslider("feedback", 0.4, 0, 0.95, 0.001) : smoo;
tone = hslider("tone", 6000, 500, 20000, 1) : smoo;

// `~` delays the loop by a frame, so the line is a frame short and the
// output a frame late: every echo lands on the time.
line = de.fdelay(MAX, max(0, time * ma.SR - 1));
core = route(4, 2, 1, 1, 2, 2, 3, 1, 4, 2) : par(i, 2, line) : ro.cross(2);
// The tone darkens the repeats, not the first echo.
process = core ~ par(i, 2, fi.lowpass(1, tone) : *(feedback)) : par(i, 2, mem);
