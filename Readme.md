small_world is a lightweight, dependency-free, geographic reference frame transformation utility.

small world supports transformations between ECEF, NED, ENU, WGS84 (latitude, longitude, height above WGS84 ellipsoid), and EGM96 (latitude, longitude, height above EGM96 geoid better known as altitude MSL). These are by no means the only geographic reference frames. They are, however, the most common for robotics.

TODO: EGM96 is not yet implemented.
TODO: Ideally small_world would support a simple, low-resolution terrain elevation model. Enough to recognize a mountain. Higher resolution is generally handled by onboard sensing and doesn't provide a lot of value. (e.g. Trees are not included in terrain elevation data. That's 30m of error right off the bat.)

Data files may be found at [earth-info.nga.mil](https://earth-info.nga.mil/index.php?dir=wgs84&action=wgs84)

Direct data links for [EGM96 15 minute interpolation grid](https://earth-info.nga.mil/php/download.php?file=egm-96interpolation), [EGM2008 2.5 minute interpolation grid](https://earth-info.nga.mil/php/download.php?file=egm-08interpolation)


I have ambitions of also handling AGL estimates with this repo. However, that would mean interacting with large datasets and geoTiff. Not impossible, but I've never done it before. Looking at the [ASTER Global Digital Elevation Model V003](https://www.earthdata.nasa.gov/data/catalog/lpcloud-astgtm-003) although USGS lists a few possible [sources and datasets](https://www.usgs.gov/faqs/where-can-i-get-global-elevation-data)

This looks like a contender. Keep in mind that it takes roughly 308GB of to represent the global elevation at a 30m resolution. https://data.naturalcapitalproject.stanford.edu/dataset/sts-632af8dc05ae810188cb2a4862f8a85022f0204daf78a040c9aa9cc248db0fd7

Alternatively, you can use this (https://search.earthdata.nasa.gov/projects?p=C1711961296-LPCLOUD!C1711961296-LPCLOUD&pg[1][v]=t&pg[1][gsk]=-start_date&pg[1][m]=download&pg[1][cd]=f&fi=ASTER&fdc=Land%20Process%20Distributed%20Active%20Archive%20Center%20(LPDAAC)&tl=1168862400!5!!). Almost 360GB