use std::sync::Arc;

use pyo3::{prelude::*, types::PyBytes, types::PyTuple};

use digest::Digest;
use tokio::sync::oneshot;

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


TODO: cache the file & mmap between functions in case we reuse it...

TODO: swap out hydrus's JSON library with something faster like https://github.com/ijl/orjson
*/

static RUNTIME: once_cell::sync::Lazy<tokio::runtime::Runtime> =
	once_cell::sync::Lazy::new(|| tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap());

struct ExtraHashes {
	md5: [u8; 16],
	sha1: [u8; 20],
	sha512: ring::digest::Digest,
}
#[pyclass]
struct FileInfo {
	file: Arc<std::fs::File>,
	mmap: Arc<memmap2::Mmap>,

	thumbnail_receiver: Option<oneshot::Receiver<()>>,
	extra_hashes_receiver: Option<oneshot::Receiver<ExtraHashes>>,
	perceptual_hash_receiver: Option<oneshot::Receiver<()>>,
	image_pixel_hash_receiver: Option<oneshot::Receiver<()>>,
}

#[pymethods]
impl FileInfo {
	#[new]
	fn new(py: Python<'_>, path: String, mime: String) -> PyResult<Self> {
		// allow_threads() to release the GIL while we do stuff...
		let (file, mmap) = py.allow_threads(|| {
			let file = Arc::new(std::fs::File::open(path)?);
			let mmap = Arc::new(unsafe { memmap2::Mmap::map(&*file)? });
			Ok((file, mmap))
		})?;

		let (hash_tx, hash_rx) = oneshot::channel();
		RUNTIME.spawn({
			let file = file.clone();
			let mmap = mmap.clone();
			async {
				FileInfo::queue_extra_hashes(&file, mmap, hash_tx).await;
			}
		});
	}
	fn get_extra_hashes<'p>(&mut self, py: Python<'p>) -> PyResult<Bound<'p, PyTuple>> {
		let extra_hashes = self.extra_hashes_receiver.take().unwrap().blocking_recv().unwrap();
		Ok(PyTuple::new_bound(
			py,
			[
				PyBytes::new_bound(py, &extra_hashes.md5),
				PyBytes::new_bound(py, &extra_hashes.sha1),
				PyBytes::new_bound(py, extra_hashes.sha512.as_ref()),
			],
		))
	}
}

impl FileInfo {
	async fn queue_extra_hashes(_file: &std::fs::File, mmap: Arc<memmap2::Mmap>, sender: oneshot::Sender<ExtraHashes>) {
		let md5_task = RUNTIME.spawn_blocking({
			let mmap = mmap.clone();
			move || md5::Md5::digest(&mmap[..])
		});
		let sha1_task = RUNTIME.spawn_blocking({
			let mmap = mmap.clone();
			move || sha1::Sha1::digest(&mmap[..])
		});
		let sha512_task = RUNTIME.spawn_blocking({
			let mmap = mmap.clone();
			move || ring::digest::digest(&ring::digest::SHA512, &mmap[..])
		});

		let _ = sender.send(ExtraHashes {
			md5: md5_task.await.unwrap().into(),
			sha1: sha1_task.await.unwrap().into(),
			sha512: sha512_task.await.unwrap(),
		});
	}
}

#[pyfunction]
fn generate_additional_hashes<'p>(py: Python<'p>, path: String) -> PyResult<()> {
	py.allow_threads(|| {
		let file = std::fs::File::open(path)?;
		let mmap = unsafe { memmap2::Mmap::map(&file)? };

		Ok::<(), std::io::Error>(())
	})?;
	Ok(())
}

#[pymodule]
fn hydrus_gubbins(m: &Bound<'_, PyModule>) -> PyResult<()> {
	m.add_class::<FileInfo>()?;
	// m.add_function(wrap_pyfunction!(generate_additional_hashes, m)?)?;
	Ok(())
}
