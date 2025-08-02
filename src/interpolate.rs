    // An example fortran implementation of bilinear and bicubic interpolation
    // can be found with the data at https://earth-info.nga.mil/php/download.php?file=egm-96interpolation

    // Another useful implementation of bilinear and bicubic interpolation may be found in the GeographicLib library
    // Specifically, the Geoid class in GeographicLib (https://geographiclib.sourceforge.io/C++/doc/Geoid_8cpp.html)
    // This is a good calculator for testing accuracy (https://geographiclib.sourceforge.io/cgi-bin/GeoidEval?input=-0.4667440%2C+0.0023000&option=Submit)
    
    pub fn cubic(lower: f32, upper: f32, x: f32, f: [f32; 4]) -> f32
    {
        // Get the pseudo index of the value in the array
        let s = ((x - lower) / (upper - lower)) * (f.len() as f32 - 1.0);
        let idx = s.floor() as usize; // nearest lower index
        let s = s - idx as f32; // Distance from the nearest lower index

        let ssq = s * s;
        let scub = s * s * s;

        let c0 = f[idx];
        let c1 = f[idx + 1];

        let cm1 = if idx != 0 {
            f[idx - 1]
        } else {
            3.0 * f[0] - 3.0 * f[1] + f[2]
        };

        let c2 = if idx != f.len() - 2 {
            f[idx + 2]
        } else {
            3.0 * f[f.len() - 1] - 3.0 * f[f.len() - 2] + f[f.len() - 3]
        };

        (cm1 * (-scub + 2.0 * ssq - s)
            + c0 * (3.0 * scub - 5.0 * ssq + 2.0)
            + c1 * (-3.0 * scub + 4.0 * ssq + s)
            + c2 * (scub - ssq)) 
            / 2.0
    }

    pub fn bicubic(x: f32, y: f32, f: [[f32; 4]; 4], eval_grid: [[(f32, f32); 4]; 4]) -> f32 {

        let x_lower = eval_grid[0][0].1;
        let x_upper = eval_grid[0][3].1;
        let y_lower: f32 = eval_grid[0][0].0;
        let y_upper = eval_grid[3][0].0;

        let mut evaluated = [0.0f32; 4];
        evaluated[0] = cubic(x_lower, x_upper, x, f[0]);
        evaluated[1] = cubic(x_lower, x_upper, x, f[1]);
        evaluated[2] = cubic(x_lower, x_upper, x, f[2]);
        evaluated[3] = cubic(x_lower, x_upper, x, f[3]);

        return cubic(y_lower, y_upper, y, evaluated);
    }

    /*#################################################################################################################
    Interpolation function implemented in GeographicLib (https://geographiclib.sourceforge.io/C++/doc/Geoid_8cpp.html)
    The calculator that uses these functions is the most correct so far as I can tell. The above bicubic and bilinear 
    are close, but more error than I like when compared to the EGM96 example points and offsets.
    ###################################################################################################################*/
//  Math::real Geoid::height(real lat, real lon) const {
//     using std::isnan;           // Needed for Centos 7, ubuntu 14
//     lat = Math::LatFix(lat);
//     if (isnan(lat) || isnan(lon)) {
//       return Math::NaN();
//     }
//     lon = Math::AngNormalize(lon);
//     real
//       fx =  lon * _rlonres,
//       fy = -lat * _rlatres;
//     int
//       ix = int(floor(fx)),
//       iy = min((_height - 1)/2 - 1, int(floor(fy)));
//     fx -= ix;
//     fy -= iy;
//     iy += (_height - 1)/2;
//     ix += ix < 0 ? _width : (ix >= _width ? -_width : 0);
//     real v00 = 0, v01 = 0, v10 = 0, v11 = 0;
//     real t[nterms_];
 
//     if (_threadsafe || !(ix == _ix && iy == _iy)) {
//       if (!_cubic) {
//         v00 = rawval(ix    , iy    );
//         v01 = rawval(ix + 1, iy    );
//         v10 = rawval(ix    , iy + 1);
//         v11 = rawval(ix + 1, iy + 1);
//       } else {
//         real v[stencilsize_];
//         int k = 0;
//         v[k++] = rawval(ix    , iy - 1);
//         v[k++] = rawval(ix + 1, iy - 1);
//         v[k++] = rawval(ix - 1, iy    );
//         v[k++] = rawval(ix    , iy    );
//         v[k++] = rawval(ix + 1, iy    );
//         v[k++] = rawval(ix + 2, iy    );
//         v[k++] = rawval(ix - 1, iy + 1);
//         v[k++] = rawval(ix    , iy + 1);
//         v[k++] = rawval(ix + 1, iy + 1);
//         v[k++] = rawval(ix + 2, iy + 1);
//         v[k++] = rawval(ix    , iy + 2);
//         v[k++] = rawval(ix + 1, iy + 2);
 
//         const int* c3x = iy == 0 ? c3n_ : (iy == _height - 2 ? c3s_ : c3_);
//         int c0x = iy == 0 ? c0n_ : (iy == _height - 2 ? c0s_ : c0_);
//         for (unsigned i = 0; i < nterms_; ++i) {
//           t[i] = 0;
//           for (unsigned j = 0; j < stencilsize_; ++j)
//             t[i] += v[j] * c3x[nterms_ * j + i];
//           t[i] /= c0x;
//         }
//       }
//     } else { // same cell; used cached coefficients
//       if (!_cubic) {
//         v00 = _v00;
//         v01 = _v01;
//         v10 = _v10;
//         v11 = _v11;
//       } else
//         copy(_t, _t + nterms_, t);
//     }
//     if (!_cubic) {
//       real
//         a = (1 - fx) * v00 + fx * v01,
//         b = (1 - fx) * v10 + fx * v11,
//         c = (1 - fy) * a + fy * b,
//         h = _offset + _scale * c;
//       if (!_threadsafe) {
//         _ix = ix;
//         _iy = iy;
//         _v00 = v00;
//         _v01 = v01;
//         _v10 = v10;
//         _v11 = v11;
//       }
//       return h;
//     } else {
//       real h = t[0] + fx * (t[1] + fx * (t[3] + fx * t[6])) +
//         fy * (t[2] + fx * (t[4] + fx * t[7]) +
//              fy * (t[5] + fx * t[8] + fy * t[9]));
//       h = _offset + _scale * h;
//       if (!_threadsafe) {
//         _ix = ix;
//         _iy = iy;
//         copy(t, t + nterms_, _t);
//       }
//       return h;
//     }
//   }
