#ifndef SMALL_WORLD_H
#define SMALL_WORLD_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum SwStatus {
  SW_STATUS_OK = 0,
  SW_STATUS_NULL_POINTER = 1,
  SW_STATUS_INVALID_ARGUMENT = 2,
  SW_STATUS_INITIALIZATION_ERROR = 3,
  SW_STATUS_QUERY_ERROR = 4,
  SW_STATUS_INTERNAL_ERROR = 5,
} SwStatus;

typedef enum SwGeoidModel {
  SW_GEOID_EGM96 = 0,
  SW_GEOID_EGM2008 = 1,
} SwGeoidModel;

typedef enum SwInterpolation {
  SW_INTERP_NEAREST = 0,
  SW_INTERP_BILINEAR = 1,
  SW_INTERP_BICUBIC = 2,
} SwInterpolation;

typedef enum SwVerticalFrame {
  SW_FRAME_AGL = 0,
  /* MSL is relative to the geoid model the converter was created with (EGM96 or EGM2008).
   * Feeding this converter MSL values referenced to a different model is a datum error the
   * C ABI cannot detect -- re-reference such values before the call. AGL conversions return
   * SW_STATUS_INVALID_ARGUMENT when the terrain dataset's vertical datum (SRTM: EGM96) does
   * not match the converter's geoid model. */
  SW_FRAME_MSL = 1,
  SW_FRAME_HAE = 2,
} SwVerticalFrame;

typedef enum SwVoidPolicy {
  SW_VOID_ERROR = 0,
  SW_VOID_ZERO = 1,
  SW_VOID_NEAREST_VALID = 2,
} SwVoidPolicy;

typedef struct SwConverterOptions {
  SwGeoidModel geoid_model;
  SwInterpolation geoid_interpolation;
  SwInterpolation terrain_interpolation;
  uint32_t terrain_cache_tiles;
  SwVoidPolicy void_policy;
  uint32_t void_policy_radius_cells;
  uint8_t preload_geoid;
} SwConverterOptions;

typedef struct SwTerrainReference {
  double geoid_offset_m;
  double ground_msl_m;
  double ground_hae_m;
} SwTerrainReference;

typedef struct SwLlaWgs84 {
  double lat_deg;
  double lon_deg;
  double hae_m;
} SwLlaWgs84;

typedef struct SwEcef {
  double x_m;
  double y_m;
  double z_m;
} SwEcef;

typedef struct SwNed {
  double n_m;
  double e_m;
  double d_m;
} SwNed;

typedef struct SwEnu {
  double e_m;
  double n_m;
  double u_m;
} SwEnu;

typedef struct SwConverterHandle SwConverterHandle;

SwStatus sw_converter_options_default(SwConverterOptions* out_options);
SwStatus sw_converter_create(const char* geoid_path,
                            const char* terrain_root,
                            const SwConverterOptions* options,
                            SwConverterHandle** out_converter);
void sw_converter_destroy(SwConverterHandle* converter);

const char* sw_last_error_message(void);

SwStatus sw_converter_convert_height_m(const SwConverterHandle* converter,
                                       double lat_deg,
                                       double lon_deg,
                                       double meters,
                                       SwVerticalFrame source_frame,
                                       SwVerticalFrame target_frame,
                                       double* out_meters);
SwStatus sw_converter_reference(const SwConverterHandle* converter,
                                double lat_deg,
                                double lon_deg,
                                SwTerrainReference* out_reference);
SwStatus sw_converter_lla_wgs84_from_height_m(const SwConverterHandle* converter,
                                              double lat_deg,
                                              double lon_deg,
                                              double meters,
                                              SwVerticalFrame source_frame,
                                              SwLlaWgs84* out_lla);
SwStatus sw_converter_ecef_wgs84_from_height_m(const SwConverterHandle* converter,
                                               double lat_deg,
                                               double lon_deg,
                                               double meters,
                                               SwVerticalFrame source_frame,
                                               SwEcef* out_ecef);
SwStatus sw_converter_height_from_ecef_wgs84_m(const SwConverterHandle* converter,
                                               SwEcef point_ecef_wgs84,
                                               SwVerticalFrame target_frame,
                                               double* out_meters);
SwStatus sw_converter_terrain_cache_stats(const SwConverterHandle* converter,
                                          uint64_t* out_cached_tiles,
                                          uint64_t* out_loaded_tiles);

SwStatus sw_wgs84_ned_to_lla(SwLlaWgs84 origin_lla_wgs84,
                             SwNed point_ned_m,
                             SwLlaWgs84* out_lla);
SwStatus sw_wgs84_lla_to_ned(SwLlaWgs84 point_lla_wgs84,
                             SwLlaWgs84 origin_lla_wgs84,
                             SwNed* out_ned);
SwStatus sw_wgs84_lla_to_ecef(SwLlaWgs84 point_lla_wgs84, SwEcef* out_ecef);
SwStatus sw_wgs84_ecef_to_lla(SwEcef point_ecef_wgs84, SwLlaWgs84* out_lla);
SwStatus sw_wgs84_ned_to_ecef(SwLlaWgs84 origin_lla_wgs84,
                              SwNed point_ned_m,
                              SwEcef* out_ecef);
SwStatus sw_wgs84_ecef_to_ned(SwEcef point_ecef_wgs84,
                              SwLlaWgs84 origin_lla_wgs84,
                              SwNed* out_ned);
SwStatus sw_wgs84_enu_to_ned_between_origins(SwEnu point_enu_m,
                                             SwLlaWgs84 enu_origin_lla_wgs84,
                                             SwLlaWgs84 ned_origin_lla_wgs84,
                                             SwNed* out_ned);
SwStatus sw_wgs84_enu_to_lla(SwEnu point_enu_m,
                             SwLlaWgs84 enu_origin_lla_wgs84,
                             SwLlaWgs84* out_lla);
SwStatus sw_wgs84_enu_to_ecef(SwEnu point_enu_m,
                              SwLlaWgs84 enu_origin_lla_wgs84,
                              SwEcef* out_ecef);
SwStatus sw_wgs84_ecef_to_enu(SwEcef point_ecef_wgs84,
                              SwLlaWgs84 enu_origin_lla_wgs84,
                              SwEnu* out_enu);

#ifdef __cplusplus
}  // extern "C"
#endif

#endif  // SMALL_WORLD_H
