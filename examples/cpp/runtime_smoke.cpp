#include <cmath>
#include <cstdlib>
#include <iostream>

#include "small_world.h"

namespace {

// Smoke test tolerance is intentionally loose enough to avoid
// cross-toolchain floating-point noise while still catching regressions.
constexpr double kTol = 1e-6;

void require_ok(SwStatus status, const char* step) {
  if (status != SW_STATUS_OK) {
    std::cerr << step << " failed: " << sw_last_error_message() << "\n";
    std::exit(1);
  }
}

void require_near(double actual, double expected, const char* label) {
  if (std::fabs(actual - expected) > kTol) {
    std::cerr << label << " mismatch: actual=" << actual
              << " expected=" << expected << "\n";
    std::exit(1);
  }
}

}  // namespace

int main() {
  const SwLlaWgs84 origin{39.0, -77.0, 150.0};

  // Same-origin ENU->NED should map axis-convention exactly.
  const SwEnu enu{15.0, -4.0, 3.0};
  SwNed ned_from_enu{};
  require_ok(sw_wgs84_enu_to_ned_between_origins(enu, origin, origin, &ned_from_enu),
             "sw_wgs84_enu_to_ned_between_origins");
  require_near(ned_from_enu.n_m, -4.0, "ned_from_enu.n_m");
  require_near(ned_from_enu.e_m, 15.0, "ned_from_enu.e_m");
  require_near(ned_from_enu.d_m, -3.0, "ned_from_enu.d_m");

  // LLA <-> NED round-trip at same origin should be stable.
  const SwLlaWgs84 point{39.0002, -77.0003, 172.0};
  SwNed ned{};
  require_ok(sw_wgs84_lla_to_ned(point, origin, &ned), "sw_wgs84_lla_to_ned");

  SwLlaWgs84 point_back{};
  require_ok(sw_wgs84_ned_to_lla(origin, ned, &point_back), "sw_wgs84_ned_to_lla");
  require_near(point_back.lat_deg, point.lat_deg, "point_back.lat_deg");
  require_near(point_back.lon_deg, point.lon_deg, "point_back.lon_deg");
  require_near(point_back.hae_m, point.hae_m, "point_back.hae_m");

  std::cout << "C++ runtime smoke passed.\n";
  return 0;
}
