use pyo3::{prelude::*, types::PyBytes, types::PyTuple};

use digest::Digest;

/*

https://github.com/PyO3/pyo3
https://docs.rs/pyo3/latest/pyo3/
https://pyo3.rs/v0.22.2/conversions/tables

https://github.com/awestlake87/pyo3-asyncio

https://github.com/PyO3/maturin
https://github.com/PyO3/maturin-action

https://github.com/PyO3/setuptools-rust


https://hydrusnetwork.github.io/hydrus/running_from_source.html

*/

/*
figure out if we can make a pyo3 package that will prefetch images for hydrus in a background thread...

see hydrus\client\importing\ClientImportFiles.py

speed up "generating additional hashes"
	hydrus\core\files\HydrusFileHandling.py
		GetExtraHashesFromPath
speed up "generating similar files metadata"
	hydrus\client\ClientImageHandling.py
		GenerateShapePerceptualHashes
speed up "generating thumbnail"?
*/

#[pyfunction]
fn generate_additional_hashes<'p>(py: Python<'p>, path: String) -> PyResult<Bound<'p, PyTuple>> {
	let mut hash_md5 = md5::Md5::new();
	let mut hash_sha1 = sha1::Sha1::new();
	// let mut hash_sha512 = sha2::Sha512::new();
	let mut hash_sha512 = None;

	// allow_threads() to release the GIL while we do stuff...
	py.allow_threads(|| {
		let file = std::fs::File::open(path)?;
		let mmap = unsafe { memmap2::Mmap::map(&file)? };

		std::thread::scope(|s| {
			std::thread::Builder::new()
				.name("hydrus_gubbins_md5".to_string())
				.spawn_scoped(s, || {
					hash_md5.update(&mmap[..]);
				})
				.unwrap();
			std::thread::Builder::new()
				.name("hydrus_gubbins_sha1".to_string())
				.spawn_scoped(s, || {
					hash_sha1.update(&mmap[..]);
				})
				.unwrap();
			std::thread::Builder::new()
				.name("hydrus_gubbins_sha512".to_string())
				.spawn_scoped(s, || {
					hash_sha512 = Some(ring::digest::digest(&ring::digest::SHA512, &mmap[..]));
					// hash_sha512.update(&mmap[..]);
				})
				.unwrap();
		});
		Ok::<(), std::io::Error>(())
	})?;

	let digest_md5 = hash_md5.finalize();
	let digest_sha1 = hash_sha1.finalize();
	// let digest_sha512 = hash_sha512.finalize();
	let hash_sha512 = hash_sha512.unwrap();
	let digest_sha512 = hash_sha512.as_ref();

	Ok(PyTuple::new_bound(
		py,
		[
			PyBytes::new_bound(py, &digest_md5),
			PyBytes::new_bound(py, &digest_sha1),
			PyBytes::new_bound(py, &digest_sha512),
		],
	))
}

#[pymodule]
fn hydrus_gubbins(m: &Bound<'_, PyModule>) -> PyResult<()> {
	m.add_function(wrap_pyfunction!(generate_additional_hashes, m)?)?;
	Ok(())
}
