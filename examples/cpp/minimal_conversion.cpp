#include <cmath>
#include <cstdlib>
#include <iostream>

#include "small_world.h"

static void require_ok(SwStatus status, const char* step) {
  if (status != SW_STATUS_OK) {
    std::cerr << step << " failed: " << sw_last_error_message() << "\n";
    std::exit(1);
  }
}

int main(int argc, char** argv) {
  const char* geoid_path = std::getenv("SMALL_WORLD_GEOID_PATH");
  const char* terrain_root = std::getenv("SMALL_WORLD_TERRAIN_ROOT");
  if (argc >= 3) {
    geoid_path = argv[1];
    terrain_root = argv[2];
  }
  if (geoid_path == nullptr || geoid_path[0] == '\0') {
    geoid_path = "data/WW15MGH.DAC";
  }
  if (terrain_root == nullptr || terrain_root[0] == '\0') {
    terrain_root = "data/srtm";
  }

  SwConverterOptions opts{};
  require_ok(sw_converter_options_default(&opts), "sw_converter_options_default");
  opts.geoid_model = SW_GEOID_EGM96;
  opts.geoid_interpolation = SW_INTERP_BILINEAR;
  opts.terrain_interpolation = SW_INTERP_BILINEAR;
  opts.preload_geoid = 1;

  SwConverterHandle* converter = nullptr;
  require_ok(sw_converter_create(geoid_path, terrain_root, &opts, &converter),
             "sw_converter_create");

  // 1) Convert an MSL altitude to absolute HAE at this geodetic point.
  SwLlaWgs84 enu_origin{};
  require_ok(sw_converter_lla_wgs84_from_height_m(converter, 39.0000, -77.0000, 110.0,
                                                  SW_FRAME_MSL, &enu_origin),
             "sw_converter_lla_wgs84_from_height_m");

  SwLlaWgs84 ned_origin{};
  require_ok(sw_converter_lla_wgs84_from_height_m(converter, 39.0005, -77.0008, 120.0,
                                                  SW_FRAME_MSL, &ned_origin),
             "sw_converter_lla_wgs84_from_height_m");

  // 2) Convert ENU at one origin to NED at another origin (both in WGS84/HAE).
  SwNed ned{};
  SwEnu enu{15.0, -4.0, 3.0};
  require_ok(sw_wgs84_enu_to_ned_between_origins(enu, enu_origin, ned_origin, &ned),
             "sw_wgs84_enu_to_ned_between_origins");

  std::cout << "ENU->NED: n=" << ned.n_m << ", e=" << ned.e_m << ", d=" << ned.d_m << "\n";

  sw_converter_destroy(converter);
  return 0;
}
