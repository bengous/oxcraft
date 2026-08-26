//! GPU-side storage of uploaded chunk meshes, drawn by the terrain and
//! water passes.

use std::collections::HashMap;

use ox_core::mesher::ChunkMesh;

use crate::gpu::Gpu;
use crate::pass::Geo;

/// Uploaded buffers for one chunk: opaque geometry plus optional water.
pub(crate) struct ChunkGpu {
    opaque: Geo,
    water: Option<Geo>,
}

impl ChunkGpu {
    /// Opaque geometry of this chunk.
    pub(crate) const fn opaque(&self) -> &Geo {
        &self.opaque
    }

    /// Water geometry, present only when the mesh has water faces.
    pub(crate) const fn water(&self) -> Option<&Geo> {
        self.water.as_ref()
    }
}

/// Every chunk whose mesh is resident on the GPU, keyed by chunk coords.
pub(crate) struct ChunkStore {
    chunks: HashMap<(i32, i32), ChunkGpu>,
}

impl ChunkStore {
    pub(crate) fn new() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }

    /// Uploads or replaces the GPU buffers for one chunk's mesh.
    pub(crate) fn upload(&mut self, gpu: &Gpu, key: (i32, i32), mesh: &ChunkMesh) {
        let opaque = Geo::mesh(&gpu.device, &mesh.opaque.vertices, &mesh.opaque.indices);
        let water = if mesh.water.is_empty() {
            None
        } else {
            Some(Geo::mesh(
                &gpu.device,
                &mesh.water.vertices,
                &mesh.water.indices,
            ))
        };
        self.chunks.insert(key, ChunkGpu { opaque, water });
    }

    /// Drops a chunk's GPU buffers.
    pub(crate) fn remove(&mut self, key: (i32, i32)) {
        self.chunks.remove(&key);
    }

    /// Coordinates of every chunk currently resident on the GPU.
    pub(crate) fn keys(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.chunks.keys().copied()
    }

    /// Resident chunks in unspecified order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &ChunkGpu> {
        self.chunks.values()
    }
}
