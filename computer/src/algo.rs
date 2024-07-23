fn validate_lt(lt: Vec<Vec<f32>>) {
	// Compute sum of incoming distrust
	for i in 0..lt.len() {
		assert!(lt.len() == lt[i].len());
		for j in 0..lt[i].len() {
			// Make sure we are not giving score to ourselves
			if i == j {
				assert_eq!(lt[i][j], 0.);
			}
		}
	}
}

fn normalise(lt_vec: Vec<f32>) -> Vec<f32> {
	let sum: f32 = lt_vec.iter().sum();
	lt_vec.into_iter().map(|x| x / sum).collect()
}

fn vec_add(s: Vec<f32>, y: Vec<f32>) -> Vec<f32> {
	assert!(s.len() == y.len());
	let mut out: Vec<f32> = vec![0.0; s.len()];
	for i in 0..s.len() {
		out[i] = s[i] + y[i];
	}
	out
}

pub fn positive_run<const NUM_ITER: usize>(mut lt: Vec<Vec<f32>>, seed: Vec<f32>) -> Vec<f32> {
	validate_lt(lt.clone());
	for i in 0..lt.len() {
		lt[i] = normalise(lt[i].clone());
	}

	let mut s = seed.clone();

	for _ in 0..NUM_ITER {
		let mut new_s = vec![0.0; seed.len()];

		// Compute sum of incoming weights
		for i in 0..seed.len() {
			for j in 0..seed.len() {
				new_s[i] += lt[j][i] * s[j];
			}
		}

		s = new_s;
	}

	s
}
