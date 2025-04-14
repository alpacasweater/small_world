small_world is a lightweight, dependency-free, geographic reference frame transformation utility.

small world supports transformations between ECEF, NED, ENU, WGS84 (latitude, longitude, height above WGS84 ellipsoid), and EGM96 (latitude, longitude, height above EGM96 geoid better known as altitude MSL). These are by no means the only geographic reference frames. They are, however, the most common for robotics.

TODO: EGM96 is not yet implemented.
TODO: Ideally small_world would support a simple, low-resolution terrain elevation model. Enough to recognize a mountain. Higher resolution is generally handled by onboard sensing and doesn't provide a lot of value. (e.g. Trees are not included in terrain elevation data. That's 30m of error right off the bat.)