# Bundled vendor headers

Definition headers of the graphics vendors' encoder interfaces. The bindings of the hardware
encoders are generated from them at build time; nothing here is compiled or linked. All of them
are MIT licensed; the licence texts are in `licenses/`.

| Directory            | Project            | Version   | Source                                                                                           | Licence text                  |
|----------------------|--------------------|-----------|--------------------------------------------------------------------------------------------------|-------------------------------|
| `include/ffnvcodec/` | nv-codec-headers   | 13.0.19.0 | https://github.com/FFmpeg/nv-codec-headers/archive/refs/tags/n13.0.19.0.tar.gz (`include/`)      | `licenses/nv-codec-headers-LICENSE` |
| `include/AMF/`       | AMD AMF            | 1.5.3     | https://github.com/GPUOpen-LibrariesAndSDKs/AMF/releases/download/v1.5.3/AMF-headers-v1.5.3.tar.gz | `licenses/AMF-LICENSE`        |
| `include/vpl/`       | Intel libvpl       | 2.17.0    | https://github.com/intel/libvpl/archive/refs/tags/v2.17.0.tar.gz (`api/vpl/`)                    | `licenses/libvpl-LICENSE`     |
| `include/va/`        | libva              | 2.22.0    | https://github.com/intel/libva/archive/refs/tags/2.22.0.tar.gz (`va/`)                           | `licenses/libva-COPYING`      |

The headers are unmodified. From libva only the public headers of libva and libva-drm are kept,
and `va_version.h` is `va_version.h.in` with the version numbers of that release filled in, as
libva's own build does. The NVIDIA and AMD licence texts are the notices from their headers,
because those archives ship no separate licence file.
