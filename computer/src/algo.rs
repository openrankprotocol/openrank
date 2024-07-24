use std::collections::HashMap;

fn pre_process(lt: &mut HashMap<u32, HashMap<u32, f32>>) {
	for (key, value) in lt {
		value.insert(*key, 0.);
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

pub fn positive_run<const NUM_ITER: usize>(
	mut lt: &HashMap<u32, HashMap<u32, f32>>, seed: &HashMap<u32, f32>,
) -> Vec<f32> {
	vec![0.0; 100000]
}
