small_world is a lightweight, dependency-free, geographic reference frame transformation utility.

small world supports transformations between ECEF, NED, ENU, WGS84 (latitude, longitude, height above WGS84 ellipsoid), and EGM96 (latitude, longitude, height above EGM96 geoid better known as altitude MSL). These are by no means the only geographic reference frames. They are, however, the most common for robotics.

TODO: EGM96 is not yet implemented.
TODO: Ideally small_world would support a simple, low-resolution terrain elevation model. Enough to recognize a mountain. Higher resolution is generally handled by onboard sensing and doesn't provide a lot of value. (e.g. Trees are not included in terrain elevation data. That's 30m of error right off the bat.)

Data files may be found at [earth-info.nga.mil](https://earth-info.nga.mil/index.php?dir=wgs84&action=wgs84)

Direct data links for [EGM96 15 minute interpolation grid](https://earth-info.nga.mil/php/download.php?file=egm-96interpolation), [EGM2008 2.5 minute interpolation grid](https://earth-info.nga.mil/php/download.php?file=egm-08interpolation)
