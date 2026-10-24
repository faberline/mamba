# Glossary

**CPython** is the reference Python implementation used by the project documents. The [project README](../README.md) describes the package manager workflow on CPython and the later runtime replacement goal.

**Package manager** is the `mamba` command path for project setup, environment creation, dependencies, and runs. The [product requirements](product/package-manager.md) define its promise.

**mambalibs** are Rust kits with Python surfaces. The [product requirements](product/mambalibs.md) say a wheel reaches CPython first. A runtime carrier comes later.

**Wheel** is the Python package form used to install the named mambalibs kits into a project environment. See the [mambalibs requirements](product/mambalibs.md).

**Runtime carrier** is the `MambaModule` form of a kit inside the mamba runtime. Its Python API must match the wheel. See the [product index](product/README.md) and [native integration guide](integration/native-runtime-integration.md).

**Binding contract** is the interface in `mamba-registry`, including `MbValue`, `MambaModule`, and `MAMBA_MODULES`. See the [integration guide](integration/native-runtime-integration.md).

**Quality** is the separate Rust quality service reached by `mamba quality`. Its command shape is in [command_quality.rs](../src/quality_command/command_quality.rs). Its current limits are in the [Quality progress record](operations/python-quality/native-progress-20261023.md).

**Conformance** means a measured match with the named reference behavior. The [source map](../llms.txt) separates self-written regression fixtures from the still pending external CPython test denominator.
