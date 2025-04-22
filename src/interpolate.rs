fn grid_sq_search<T: PartialOrd>(vec: &[T], val: T, ascending: bool) -> isize {
    if vec.is_empty() || val < vec[0] || val > vec[vec.len() - 1] {
        return -1; // Out of bounds; extrapolate here
    }

    // Choose the comparison logic based on whether the vector is sorted ascending or descending
    let compare = if ascending {
        |a: &T, b: &T| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
    } else {
        |a: &T, b: &T| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
    };

    match vec.binary_search_by(compare) {
        Ok(i) => {
            if i == 0 {
                0
            } else {
                (i - 1) as isize
            }
        }
        Err(i) => {
            if i == 0 {
                0
            } else {
                (i - 1) as isize
            }
        }
    }
}


/* Interpolation is based on the presentation in
 * Chapter 3 of Numerical Recipes in C: The Art of Scientific Computing (ISBN
 * 0-521-43108-5) [herein referred to as NR in C] with Eigen and Templates.
 *
 *
 * Naming convention matches Numerical Recipes in C to be a bit clearer.
 *
 * Note the calling conventions are un-matlab-y  for 2d (opposite of matlab)
 *
 * User is responsible for all input validation; for example, if the number of
 * points in the input vectors is different.
 */

/* Assuming vec is sorted in *strictly ascending* order , find the i such that
 * vec[i] <= val < vec[i+1]
 */
template <typename Tv, typename Tdata>
int grid_sq_search(const Tv& vec, Tdata val) {
	/* Get a pointer to the eigen data */
	const Tdata* data = vec.data();
	const Tdata* lb = std::lower_bound(data, data + vec.size(), val);
	/* lb points to the first element of vec's data s.t. *lb >= val
	 */

	if (val < vec(0) || val > vec(vec.size() - 1)) {
		return -1;
		/* Out of bounds; extrapolate here */
	}

	/* if lb is *exactly* data (can happen for integral data, highly unlikely for
	 * floating point) */
	if (lb == data) return 0;

	/* Otherwise, *(lb-1) < val by definition*/
	return (lb - data) - 1;
}

/* 1-d linear interpolation, 3.3.1 and 3.3.2 in NR in C
 * Just takes an appropriate convex combination of the two adjacent points
 */

template <typename Tv, typename Tdata>
Tdata Interp1_linear(const Tv& x, const Tv& y, Tdata x1, Tdata sentval) {
	int j = grid_sq_search(x, x1);
	if (j == -1) return sentval;
	Tdata A = (x(j + 1) - x1) / (x(j + 1) - x(j));
	return A * y(j) + (((Tdata)1) - A) * y(j + 1);
}

template <typename Tv1, typename Tv2, typename Tm, typename Tdata>
Tdata Interp2_bilinear(const Tv1& x1a, const Tv2& x2a, const Tm& ya, Tdata x1,
                       Tdata x2, Tdata sentval) {
	int j = grid_sq_search(x1a, x1);
	int k = grid_sq_search(x2a, x2);
	// std::cout << " Interp 2 :" << j << "\t" << k << std::endl;
	if (j == -1 || k == -1) {
		return sentval;
	}

	Tdata y1 = ya(j, k);
	Tdata y2 = ya(j + 1, k);
	Tdata y3 = ya(j + 1, k + 1);
	Tdata y4 = ya(j, k + 1);

	Tdata t = (x1 - x1a(j)) / (x1a(j + 1) - x1a(j));
	Tdata u = (x2 - x2a(k)) / (x2a(k + 1) - x2a(k));

	constexpr Tdata td1 = (Tdata)1;
	return ((td1 - t) * (td1 - u) * y1 + t * (td1 - u) * y2 + t * u * y3 +
	        (td1 - t) * u * y4);
}

// Cubic interpolation on a uniformly spaced interval [a,b]
// We follow the discussion of Eq. 25 in
// R. G. Keys , Cubic Convolution Interpolation for Digital Image Proessing
// IEEE Trans. ASSP-29, No. 6, Dec. 1981, 1153-1160
// Cause its easier than doing whatever cubic interpolation is in Numerical
// Recipes in C. This procedure is still third order optimal due to the boundary
// conditions, so it should be fine. (under smoothness assumptions, which are
// clearly violated in this case)
//
//
// Assume grid is uniformly spaced (this is CRITICAL -- if grid is not uniformly
// spaced, the results will be garbage).
//
// User is responsible for ensuring at least 4 points in f.
//
// Note that this is the same as Matlab R2017b's v5cubic option
//
// The regular cubic option is equivalent to pchip, which is a shape preserving
// interpolation as in Monotone Piecewise Cubic Interpolation by Fritisch and
// Carlson, SIAM J. Numer. Anal., Vol 17, No 2, April 1980 238-246 We don't
// implement this (Interp1_cubic_conv exists only to implement bicubic
// interpolation)
template <typename Tdata, typename Tv>
Tdata Interp1_cubic_conv(Tdata a, Tdata b, Tdata x, const Tv& f,
                         Tdata sentval) {
	int n = f.size();
	if (x <= a || x >= b) {
		// At the endpoints, return the exact value

		if (x == a) return f(0);
		if (x == b) return f(n - 1);

		// Otherwise out of range
		return sentval;
	}

	Tdata s = ((x - a) / (b - a) * (n - 1));
	int idx = (int)s;
	s = s - idx;  // Get the part you're off from the nearest integer point.

	Tdata ssq = s * s;
	Tdata scub = s * s * s;

	Tdata cm1, c0, c1, c2;
	c0 = f(idx);
	c1 = f(idx + 1);

	// handle case where only pt smaller than x is @ a
	if (idx != 0) {
		cm1 = f(idx - 1);
	} else {
		cm1 = 3 * f(0) - 3 * f(1) + f(2);
	}

	// handle case where only pt bigger than x is @ b
	if (idx != n - 2) {
		c2 = f(idx + 2);
	} else {
		c2 = 3 * f(n - 1) - 3 * f(n - 2) + f(n - 3);
	}

	return ((cm1 * (-scub + 2 * ssq - s) + c0 * (3 * scub - 5 * ssq + 2) +
	         c1 * (-3 * scub + 4 * ssq + s) + c2 * (scub - ssq)) /
	        ((Tdata)2));
}

template <typename Tdata, typename Tv>
Tdata Interp1_cubic_conv(const Tv& x, const Tv& y, Tdata x1, Tdata sentval) {
	return Interp1_cubic_conv(
	    (Tdata)x(0), (Tdata)x(x.size() - 1), x1, y,
	    sentval);  // Theres probably a terser way to say end of vector?
}

// In order to do bicubic interpolation with cubic convolution interpolation
// we interpolate 2 before / 2 after
// with boundary checking.
//
// Should match Matlab's cubic interpolator with interp2.
template <typename Tm, typename Tdata>
Tdata Interp2_bicubic(Tdata a1, Tdata b1, Tdata a2, Tdata b2, const Tm& ya,
                      Tdata x1, Tdata x2, Tdata sentval) {
	Eigen::Matrix<Tdata, 4, 1> interpval;

	// First, check that x1, x2 are within the appropriate intervals
	if (x1 < a1 || x1 > b1 || x2 < a2 || x2 > b2) {
		return sentval;
	}

	int row_idx = (int)((x1 - a1) / (b1 - a1) * (ya.rows() - 1));
	int first_row = row_idx - 1;
	int second_row = row_idx + 2;

	if (first_row <= 0)  // Only 1 element before
	{
		first_row = 0;
		second_row = 3;
	}
	if (second_row >= ya.rows() - 2)  // Only 1 element after
	{
		second_row = ya.rows() - 1;
		first_row = second_row - 3;
	}

	interpval(0) = Interp1_cubic_conv(a2, b2, x2, ya.row(first_row), sentval);
	interpval(1) = Interp1_cubic_conv(a2, b2, x2, ya.row(first_row + 1), sentval);
	interpval(2) = Interp1_cubic_conv(a2, b2, x2, ya.row(first_row + 2), sentval);
	interpval(3) = Interp1_cubic_conv(a2, b2, x2, ya.row(first_row + 3), sentval);

	Tdata beginning, end;
	Tdata h = (b1 - a1) / (ya.rows() - 1);
	beginning = a1 + h * first_row;
	end = a1 + h * second_row;

	return Interp1_cubic_conv(beginning, end, x1, interpval, sentval);
}

// This is a bicubic interpolation type routine.
// Assumes x1a , x2a are uniformly spaced, but this interface is provided match
// the Interp2 bilinear.
template <typename Tv1, typename Tv2, typename Tm, typename Tdata>
Tdata Interp2_bicubic(const Tv1& x1a, const Tv2& x2a, const Tm& ya, Tdata x1,
                      Tdata x2, Tdata sentval) {
	Tdata a1 = x1a(0);
	Tdata b1 = x1a(x1a.size() - 1);
	Tdata a2 = x2a(0);
	Tdata b2 = x2a(x2a.size() - 1);

	return Interp2_bicubic(a1, b1, a2, b2, ya, x1, x2, sentval);
}

// Specify a default interplation as bicubic for 2d, linear for 1d

template <typename Tv1, typename Tv2, typename Tm, typename Tdata>
Tdata Interp2(const Tv1& x1a, const Tv2& x2a, const Tm& ya, Tdata x1, Tdata x2,
              Tdata sentval) {
	return Interp2_bicubic(x1a, x2a, ya, x1, x2, sentval);
}
template <typename Tv, typename Tdata>
Tdata Interp1(const Tv& x, const Tv& y, Tdata x1, Tdata sentval) {
	return Interp1_linear(x, y, x1, sentval);
}
