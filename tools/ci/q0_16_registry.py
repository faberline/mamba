"""Build the static, tiered #16 inventory. This does not run any target tool.

The four tiers have different units. No sum of their denominators is a case
count. A source pointer proves identity only. It does not prove execution.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
from typing import Any

TARGETS = ("3.12", "3.13", "3.14")
PROFILE = "crates/mamba-quality/assets/profile.json"
INFRA = "crates/mamba-quality/src/infrastructure/"
OUTPUT = "docs/operations/python-quality/q0-16-required-behavior-registry.json"
ROLES = ("hit", "non_trigger", "interaction", "dormant", "unsupported")
# These are fixed source identities, not a list inferred from a glob. The same
# Pydantic config source has two consumers and therefore two manifest entries.
MANIFEST_SPECS = [{'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/ast_errors.json',
  'sha256': 'a0b12b310bc42f30fa2fa0d34a725ab2d5ceca808a8c9c01352a4450d1f0689f',
  'container': []},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/fixed_points.json',
  'sha256': '2d03da18d943d9bb102120f8bd59975f4dbba97c681c1d20cff0fba44932223f',
  'container': ['cells']},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/insertion_markers.json',
  'sha256': '03619fe9d5f3eda3c7f972cfa4910d1c664eae0c858e5cf0664ed94ecaed2a17',
  'container': ['cases']},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/literal_errors.json',
  'sha256': 'bb418db3b3406e93a558cff76151a2d352a83783b41ecb9aca6adc8a8e0279a7',
  'container': []},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/literal_interactions.json',
  'sha256': 'ae12343033e04cfb7eb8befe7d5d2c5605c4c0aebac8713efd59d446560b6acb',
  'container': []},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/literal_syntax.json',
  'sha256': '07a5299dfe980a067ea79ebf7467f610004adb28a9bb5f52a57703f82273484d',
  'container': []},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/literal_values.json',
  'sha256': 'cd90e8eacb285de2aa6397e511a53396f88e8ae7c9130f15402527f79d98e083',
  'container': []},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/public_cr_stream.json',
  'sha256': 'e541c5fa79a96c004deb9dcd047cab9158661d8108285021d1cd712aaaecc11c',
  'container': ['cells']},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/split_boundaries.json',
  'sha256': 'a2a6826efc10662afe6cbea32f906e403207cfebde2d604e5651a6c33b37844b',
  'container': []},
 {'tool': 'isort',
  'path': 'crates/mamba-quality/src/infrastructure/isort/tests/unicode_data.rs',
  'sha256': '32bee5864743d06e3fabecb6033bb011a1bb83d0ccb1a2c7591e9cf1fefb78dd',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/ast_constant_controls.json',
  'sha256': '08e3a2d93ebbe4989c58eab4969b2b76ea82d6f93615e59de4f56721415309f6',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/ast_setter_context.json',
  'sha256': '4ca92dcdae54af59dbcea2404c10481a61d6f7c32b42de5420b6d99734b2c9bd',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/async_iteration.json',
  'sha256': '158557942e369d31142b33b77cb323d5fd62788b4badebe0ef7b867e45430af6',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/async_iteration_protocol_controls.json',
  'sha256': 'e44d3c62499145ff598b9a75f7b378ce8e1f72f5b6fafb6838d476087095faa3',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/builtin_values.json',
  'sha256': 'eaa4e19f07cefb46a527c3f9aed7a11549e779c6a206b6cfc5abf794b203de92',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/builtin_values_regressions.json',
  'sha256': '9254f5a4c37a3a49b6770b75bf37bdde759b0995c5b11baae2ca9a8a554991c1',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/call_span_boundaries.json',
  'sha256': '955ac8fc844f760fadc2cc04f6808a076cfdbaff8ecba2defb8c70210e531fe9',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/callable_originals.json',
  'sha256': '20e83b2a158050e85b489ceed986ec6554fccccb10244e0d30b7d9dcb7a36ce5',
  'container': ['cases']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/chained_assignment_controls.json',
  'sha256': '0bf37a94d222ad4fdc4888b9e0659fe504089a73fdd26b0b6f10eefbf57bfb85',
  'container': ['cases']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/closed_constructors.json',
  'sha256': '052fd4dd951792c746563a16c20965463e0428588ad0d3197a884eea302da0a3',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/decorator_boundaries.json',
  'sha256': '6652dc3df8e2b49f35a2dc2a6bced6f828521168cedc6ee3ba1d43e3e124c0d9',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/dependencies.json',
  'sha256': '4f1ce49dc9ef108184ae7f227b68e46252464aa461ff80d895adcf91b9742e6a',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/dependency_directives.json',
  'sha256': '5293ef44013417dfa9bfa227454148620ef655f7306d5c26ad5e930fcc1db2c7',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/dependency_error_interactions.json',
  'sha256': '4ecaa74fbb18408617056d42e4a8acc8f8af178bc7d94f1d68043909eb685980',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/dependency_source_errors.json',
  'sha256': 'ed722a36b5e0c0b19a9042a58d8911589e761f9a7d484569bad068f4ce138bd6',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/duplicate_modules.json',
  'sha256': '2d90076db59404c5c0657436fcc3aceca5d2f85d085d4e6e5580a0d2c7686709',
  'container': ['cases']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/filtered_set_comprehensions_oracles.json',
  'sha256': '9a1e320bbfdbf6c9a6a546ba1a5b7391cb98e80621a12a85cf3626c4223bd070',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/filtered_set_comprehensions_sources.json',
  'sha256': '139bf65e6a5bda588bc46c6003aedd853021889f5eed71c6aab596c53089b7af',
  'container': ['inputs']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/fstring_flow_boundaries.json',
  'sha256': '952d355caf7ea17a2c2ac9f7d5206c5269842fcf27fb8dcd7a7d25acaf0d8ecc',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/generic_starred_calls.json',
  'sha256': '73478e2312f3482e460bb9551f06d24563fdb88039b496453715a8abcf024c09',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/json_file_functions.json',
  'sha256': '10865aed04e7be7da6d163a78657fe8f0e38326f90f2096ce628487a955ab75a',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/lambda_binding.json',
  'sha256': '258adae7ca61e0d5bc6740c82d65c69073e0023e0306481e538a07cc02d8ee44',
  'container': ['source']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/lambda_scope_boundaries.json',
  'sha256': '322e31022b626fe404ae180066b4f5599256690f06ff7be2e4f5da3fca1aec13',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/local_dependencies.json',
  'sha256': '0780914029319b0332fdfd4a9e897dbf03afbb15fc9d2dff2c2054bc025ab77c',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/math_binding_controls.json',
  'sha256': '48c2d24d45f9fa89f3e6eb2e820f4538ca9ec4b0fdcf1ec49945a242d89ccae1',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/math_functions.json',
  'sha256': '965c5573ec243b54c329c2805495eda70c180efbdef82bbf228047429d1ea9fc',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/name_directives.json',
  'sha256': '93d358feaf98193efce987a56fdaf6c2fe61d9f1e81f87d6a0c72578081123e4',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/names.json',
  'sha256': '3db9421fd724f50a586e96dc178cb5e8a9acde600d1256353a6026b8de353894',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/narrowing.json',
  'sha256': '131dc981ee47ddf7a53dfdc9d249879c1f7cddf89704ced2f77ce71f6ca9bfec',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/ordinary_decorator_boundaries.json',
  'sha256': '3b937b00c77d5a07802d73d7ca82657afd97cfd29fd72327fa2036716309ceda',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/ordinary_decorators.json',
  'sha256': '2fea0a802230b1001ae6f8e84175d77174703d33bb64a7fdc81bf2ccf7c1cc88',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/overload_annotations.json',
  'sha256': '76c34c55bd1d48054fe795d9c4e29af33d10c80f9a16df128d8e25fcf10c5f41',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/patterns.json',
  'sha256': '22c14384123ace01c46bc2f4487aa0eb348ed6cee6c2ec65f4ab2610d7f19478',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/predicate_bindings.json',
  'sha256': '936ee8b7ff12dc2bd510b9b20eb31764ed17b51c09788bc482c88b8d94eb9ba7',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/predicate_contracts.json',
  'sha256': '66dabf23df6026a55abe09afcdebd0a71316d43a7b78ce3dab47ffb2d0dc47ed',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/pydantic_config.json',
  'sha256': '46f933fcd2a615f806de44810680715a21d755a0556654fe3de531ef9b1c2a9f',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/pydantic_config_entrypoints.json',
  'sha256': 'a3f6f0073c49abc68b5bf44c4efef9aa26cb30cfd1445687e65f8cb58b6dfb26',
  'container': None},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/runtime_narrowing.json',
  'sha256': 'd1bb519f6f7905581728c86642d5a5ffed02577669e3d40f0bd421b3d2112a1c',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/set_comprehensions_oracles.json',
  'sha256': 'e58e27597d37d1e79ed258f8f5841b8173581165412386312ff309c3eff43e2d',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/set_comprehensions_sources.json',
  'sha256': 'c4c0782f88b18d61804a686afa2ad2139ee1d17757fe92706ac71e9e9bc49bbb',
  'container': ['inputs']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/shadow_aliases.json',
  'sha256': '1ff72ea5736feddbbc11ffac8036007d700cafd53ac02e0eed1c9cac0b87d3ee',
  'container': ['cases']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/special_decorators.json',
  'sha256': '20d676f111da1d19e503d7c21db7bb3a6b3ed46ba456265b4b9a96796e30d4b8',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/standard_functions.json',
  'sha256': 'fa880365732c79a781883620c9a4d339127a7dd9497488016e47dc83a016b502',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/standard_shadows.json',
  'sha256': '5e6b3194c7f5a6e60d8f5d1fb226538fa4aca3c2f0e5746e22d72e8db9933e4e',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/starred_call_controls.json',
  'sha256': '17d0fa24a629a5753f8c50c8ba777c58218e00e1c1cbd91b2497e82e9ae2fd58',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/stdlib_io_protocols.json',
  'sha256': 'e6f7e8262872817c398af63d2644b794b4ac67aaad7f53f07ceed1595ab11f7c',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/str_bytes_safe.json',
  'sha256': 'fec51875789e48588fa14ae9bf52ab0439da20a472d2988e8ce1ccecffa9b439',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/string_joins.json',
  'sha256': 'ed674c9fc07733cc5d412fc8e9a6c3b4db412d38cf36b9b3aa673d8fbf0e64d1',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/template_strings.json',
  'sha256': '05905d96e203e050ba0234d813c0f8b94422946a319c1fa7e459903afc57e610',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/truthiness.json',
  'sha256': 'e8dff189fd5648ae9c21c56f1a81602a0501b98adce4d7336b30cf0a5d22ebcc',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/tuple_assignment_edges.json',
  'sha256': '40fdb87bf17cd4eb5d66d24eafcbddd7a3809a684edd523f56721014e57dfc19',
  'container': ['cases']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/tuple_assignment_order.json',
  'sha256': '94b7c98358b6ed9a751a4502ca3d50930bb9f3c2237469a0650cf095caf3d091',
  'container': ['cases']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/tuple_binding_controls.json',
  'sha256': 'a9034022bd960d7923d7dfe8b23311d46f6d073570f8343dab8b6457f2e4c233',
  'container': ['cases']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/tuple_binding_oracles.json',
  'sha256': 'b36ce72cc348691eae849452d62f2dca70dff5023070a4fa6438db5d802c6585',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/typeddict_unpack.json',
  'sha256': 'f6ec24cc60c610144fd0c7f8bb197d971a1edaba231eea5abc2e2cad25023274',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/typing_cast.json',
  'sha256': '25cc04ba3a1368cf6d30cce19f37179b6b674feb4f001812b58bff54453ecd01',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/typing_cast_error_controls.json',
  'sha256': 'dacdd04724ea8ed8975e84d6de9e8b2e3715226cfe1b911aac15f17e18c26fd4',
  'container': []},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/union_boundary.json',
  'sha256': 'aeb965392b0b204db7040cf8f3e7589f021ff28ee9d74619274e9c7cf0e496e7',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/union_known_members.json',
  'sha256': 'd9c426e7d057e13f7aac9ef8a3a5cd9a09fd38bf27333fb848f9b07887b31c95',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/union_missing_members.json',
  'sha256': 'cf716eda6b27258bb1203e909e818a4b6bc8a730968bd537129ea942632b12dc',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/update_operator_controls.json',
  'sha256': 'ff47dd116e5fd25ee9ce916cce7a09b2d0055d9c75881ee56b3c0317a5978f47',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/with_binding.json',
  'sha256': '9fa4e86ee1495cd05e02426aa73fa06bf63cdb4eb4b9dcb6229ca6577a53403b',
  'container': ['source']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/with_boundaries.json',
  'sha256': 'c61286ec7dec6806caa1b8745d44fd1b904d909a9d0d6d43bc96f8d7f76dbc1e',
  'container': ['cells']},
 {'tool': 'mypy',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/with_protocol.json',
  'sha256': '1b5659203fb3a72d1580a857abd872ade4cc21efc800e1b2ff4b9f26a91069a7',
  'container': ['cells']},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/build_probes.json',
  'sha256': '30bdea026db2d70d6747a4a57d62871eae28ecc0b4b43465993fcee2fa933cf3',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_dependencies.json',
  'sha256': '1fa39a1c62e4900ccf0586825bcf05b9107e42f19942a575006335162046437e',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_dependency_regressions.json',
  'sha256': '304a591faa45c7b72e99a9d097a6ad488a37a1cf026338f3caf1710141d478e1',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_import_precedence.json',
  'sha256': 'a1337025cd1200a762fc0b1174e0715fd3aebfdb1bd86f4fa21e194c9b09df0b',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_imports.json',
  'sha256': '4c1ae99e043285ef042ffe2aa921cce4aae01912d7b52e26d7bc0eeed6ff5933',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_interactions.json',
  'sha256': '1d2643ca0436b735a9bd86f0f4a76d1fd79db88618fbf7399913cdb5690315df',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_mutations.json',
  'sha256': '439fc3405ea4b13723bb8160a2a1e06a3714f6c5d6b547b3da12ea900ffccbbb',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_names.json',
  'sha256': 'e61dcecb053dd29d0a7312440da9e0a0bfd16d208fde9a425c15030da5258b4f',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_relative_boundaries.json',
  'sha256': '71b0957dd2076a8fb449ac72cddbef4de67154770a6d2c616021a1057310561f',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_selected_imports.json',
  'sha256': '1bd3ffc68fb2ec60b28ac054e75962648f8c4ea1369384bc37fe5ab3413c4aa4',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_stores.json',
  'sha256': 'd05a2df69013aa1ddd29ca4ec00f86a25beef8b7a91271205c3fdc99ac08fde8',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/cross_file_stubs.json',
  'sha256': '7cb78fd5eb19dd30f4e2d88c293f82ac3c9f37e7ba1c0557e006726546253305',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/dependency_io.json',
  'sha256': '8854d68630e75c85af15bbdb478d5a167cf2ef23bda5bcb9a15fa92e45ef06e1',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/encoding_controls.json',
  'sha256': 'dde6e129f89073cb96afb1ce5bde2e2dca0afc9da42f3aaae9f541c69d8a08a6',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/encoding_declaration_controls.json',
  'sha256': '6814625ad428a0988ed28f66f5f83b49fc553a40a64b901d44dd41492b497df5',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/entry_inputs_data.json',
  'sha256': '6c2a88c61f024a2bec82d0f988e3072dd63d3b09056a3b83a68f8eefb1467f8b',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/entry_io_data.json',
  'sha256': 'eb0082ab0bd0d61e2fa4aad27aa0fc04a1f7bb540f42dd6874279fd0678e6fb1',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/entry_path_errno_data.json',
  'sha256': 'f13b65f99d2de6ff267bb8a3bb76d3f990405b7c523facc4693f0243bc405a3c',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/imported_function_aliases.json',
  'sha256': '269e9b226843f070ba65bde66b36a24827cbded12fcbbe53c5616b58001e1489',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/length_controls.json',
  'sha256': '4cbec8572dcc2b792cae6ea133ba3e20fe45e3cc89314afa9e8aeaabdba7000a',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/starred_comprehensions.json',
  'sha256': 'ce8cdf2279ed00ce2bd597fb085ca3010b8dabf8628b8695de6401bcef3458c2',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/terminal_reference_cycles.json',
  'sha256': 'f9a8de0c9df24f832816e9a706b4cf8a2f325696536643e690c59efd2a3f3175',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/unicode_notes.json',
  'sha256': '4834c12073fa13a42665c40262c616b94099f5681f494a995de6e7ce2d74a99d',
  'container': []},
 {'tool': 'pylint',
  'path': 'crates/mamba-quality/src/infrastructure/pylint/tests/width_markers.json',
  'sha256': 'f4442bc2ec8aae410e1c2c54676cb0ca6eb99b879bb5559f8ff1b57f622a07e6',
  'container': ['cells']},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/byte_controls.json',
  'sha256': 'e606bca84ab7fc3778dd21f19b78ed1d12e1c194bd5fa87b490886518ade8a9d',
  'container': ['cells']},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/comment_controls.json',
  'sha256': '7a694c79aa7483b91c7598e522b7d2beeba58373680be59c35857702fa8290cf',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/errors.json',
  'sha256': '7218e422eeea947e4453b8d0485dd97e9723362d96736d0be928c6e59ed8882e',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/historical_errors.json',
  'sha256': '4054535f35b613e52a6e0cd7cf5280f6cdf630ce47f29c64091b6d4c96416b66',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/interpolation_comments.json',
  'sha256': 'd569bb3f888666b17c656a6af84a30d1ff194c6d9909cf492f71c88842b11a72',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/interpolation_continuation.json',
  'sha256': '85a40b7317fbd21c315643b2301434cbae261ff9a9188605d763ec41add7d735',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/interpolation_prefixes.json',
  'sha256': '3e6fbfeb72f5f7a93cc73fe7cad96eb0e7ce247de6aaf4a76a9ddc0c85e4d4cc',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/literal_skip_controls.json',
  'sha256': '76196474ce15efc4a51216084a372ca7b917ff0dbef6637e44d0932efbec519a',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/mixed_byte_braces.json',
  'sha256': '253a6498da00c066ec49f907be6b15a7f7fc63698b63d5f3eb7b84a6a14dc403',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/mixed_error_priority.json',
  'sha256': '42f66063204d72dbd77ca4b81df25f414e17bafb99bddca1eec96d51b294cb40',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/mixed_literal_order.json',
  'sha256': '45b1e763c85caa93832632e2edf2ca19a79f9e2f03b05758963643fe3f5d822e',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/safety.json',
  'sha256': '8eb09a56b9c7f1816f7b7888eb9fca9ef60fcb5480cf32afb9b76c70c025882b',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/safety_logs.json',
  'sha256': 'f51838b87eb8d83f6b395eaaff957014f0ce2c09e22f4055b7d9597d6302cd1d',
  'container': ['cells']},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/string_directives.json',
  'sha256': '50954e54bb3fea1b361c427c50a3e5cd2c436ec0cdd3a8a0e8f0f9ab64c01e22',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/syntax_order.json',
  'sha256': '0e61d2c0c0aaf1014de6e22b80c1c881efad4bd7f2664eda2ce1f76c595efacf',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/template_grammar.json',
  'sha256': '89300387bd9e88256773affd1347b5e58ef187205e395fb4a0dc674ffbb831bf',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/template_mixed_literals.json',
  'sha256': '60b2d993c4f162fcbbea4f88b990bf3313a1366505c2e2446e975bf4f603716f',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/type_comment_controls.json',
  'sha256': 'e10f3067b0321eafc2588892b0db898635d1bbc479f639ff2f219b10897d8214',
  'container': ['controls']},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/width_data.rs',
  'sha256': 'aae19d836fffc0df4b851fc9c10f96e17cdcdffea0a83c34270f85eabab3e4cb',
  'container': []},
 {'tool': 'black',
  'path': 'crates/mamba-quality/src/infrastructure/black/tests/type_comments_data.rs',
  'sha256': '475963b599eb4c89bc7710c9a9deb391a376f5f5b3d13645f77c7e70afd50e60',
  'container': []},
 {'tool': 'pydantic',
  'path': 'crates/mamba-quality/src/infrastructure/mypy/tests/pydantic_config.json',
  'sha256': '46f933fcd2a615f806de44810680715a21d755a0556654fe3de531ef9b1c2a9f',
  'container': []}]


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def encode(value: Any) -> bytes:
    """Match the legacy record hash serialization, including its key order."""
    return json.dumps(value, ensure_ascii=True, separators=(",", ":")).encode()


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def relative_path(value: str) -> str:
    path = PurePosixPath(value)
    require(not path.is_absolute() and ".." not in path.parts and str(path) == value,
            "source path must be repository relative")
    require(bool(value) and "\\" not in value, "invalid source path")
    return value


def pointer(parts: list[Any]) -> str:
    return "".join("/" + str(part).replace("~", "~0").replace("/", "~1") for part in parts)


def select(value: Any, parts: list[Any]) -> Any:
    for part in parts:
        require(isinstance(value, (list, dict)), "pointer crosses a scalar value")
        value = value[part]
    return value


def parse_constant(text: str, name: str = "CASES") -> Any:
    match = re.search(r"pub const " + re.escape(name) + r': &str = r(?P<h>#+)"(?P<body>.*?)"(?P=h);', text, re.S)
    require(match is not None, "missing raw JSON constant " + name)
    return json.loads(match["body"])


class Sources:
    """Read only source data. A source fact is never an execution receipt."""
    def __init__(self, repository: Path):
        self.repository = repository.resolve()
        self.data: dict[str, bytes] = {}
        self.objects: dict[tuple[str, str | None], Any] = {}
        self.facts: dict[str, dict[str, Any]] = {}

    def read(self, path: str, expected_sha256: str | None = None) -> bytes:
        path = relative_path(path)
        if path not in self.data:
            real = (self.repository / path).resolve()
            require(real.is_relative_to(self.repository), "source escapes repository")
            raw = real.read_bytes()
            self.data[path] = raw
            self.facts[path] = {"source_id": path, "path": path,
                                "bytes": len(raw), "sha256": digest(raw)}
        raw = self.data[path]
        if expected_sha256 is not None:
            require(digest(raw) == expected_sha256, "source hash differs: " + path)
        return raw

    def json(self, path: str, constant: str | None = None) -> Any:
        key = (path, constant)
        if key not in self.objects:
            raw = self.read(path).decode()
            self.objects[key] = parse_constant(raw, constant) if constant else json.loads(raw)
        return self.objects[key]

    def ref(self, path: str, parts: list[Any] | None = None,
            constant: str | None = None) -> dict[str, Any]:
        self.read(path)
        result = {"source_id": path, "sha256": self.facts[path]["sha256"],
                  "json_pointer": pointer(parts or [])}
        if constant:
            result["constant"] = constant
        return result

    def value_ref(self, path: str, parts: list[Any], constant: str | None = None) -> dict[str, Any]:
        value = select(self.json(path, constant), parts)
        return {**self.ref(path, parts, constant), "value_sha256": digest(encode(value))}


def unrun() -> dict[str, str | bool]:
    return {"reference_execution_status": "not_run_as_own_contract",
            "native_product_status": "not_run", "accepted_role_witness": False,
            "source_hash_overlap_is_execution_proof": False}


def owner_bindings(sources: Sources, path: str) -> list[dict[str, Any]]:
    """Find include bindings and the three Rust module bindings as source facts."""
    file = sources.repository / path
    bindings = []
    if file.suffix == ".rs":
        modules = {
            "isort/tests/unicode_data.rs": ("isort/tests.rs", "mod unicode_data;"),
            "black/tests/width_data.rs": ("black/tests.rs", "mod width_data;"),
            "black/tests/type_comments_data.rs": ("black/tests/type_comments.rs", "super::type_comments_data::CASES"),
        }
        key = path.removeprefix(INFRA)
        if key in modules:
            owner, needle = modules[key]
            candidates = [(sources.repository / (INFRA + owner), needle)]
        else:
            candidates = []
    else:
        pattern = re.compile(r'include_(?:str|bytes)!\("' + re.escape(file.name) + r'"\)')
        candidates = [(p, pattern) for p in sorted(file.parent.glob("*.rs"))]
    for owner, needle in candidates:
        relative = owner.relative_to(sources.repository).as_posix()
        text = sources.read(relative).decode()
        lines = text.splitlines()
        hits = [i + 1 for i, line in enumerate(lines)
                if (needle in line if isinstance(needle, str) else needle.search(line))]
        if not hits:
            continue
        # An expected array length is not target evidence. Require an explicit
        # ordered owner axis. In particular, width_data has no such axis.
        axis = re.search(r'(?:PythonVersion|PythonTarget)::Py312\s*\)?\s*,.{0,300}?(?:PythonVersion|PythonTarget)::Py313\s*\)?\s*,.{0,300}?(?:PythonVersion|PythonTarget)::Py314', text, re.S)
        record = {"source_id": relative, "sha256": sources.facts[relative]["sha256"],
                  "binding_lines_1based": hits, "binding_kind": "module" if isinstance(needle, str) else "include"}
        if axis:
            record["target_axis"] = list(TARGETS)
            record["target_axis_lines_1based"] = [text.count("\n", 0, axis.start()) + 1,
                                                    text.count("\n", 0, axis.end()) + 1]
        bindings.append(record)
        delegated = re.search(r'super::([a-z_]+)::compare\(\s*include_str!\("' + re.escape(file.name) + r'"\)', text)
        if delegated:
            helper_path = owner.parent / (delegated[1] + ".rs")
            helper_relative = helper_path.relative_to(sources.repository).as_posix()
            helper = sources.read(helper_relative).decode()
            helper_axis = re.search(r'PythonVersion::Py312\s*,\s*PythonVersion::Py313\s*,\s*PythonVersion::Py314', helper)
            require(helper_axis is not None and 'let expected = &case["expected"][target];' in helper,
                    "delegated expected target axis is not source bound")
            bindings.append({"source_id": helper_relative,
                "sha256": sources.facts[helper_relative]["sha256"],
                "binding_kind": "delegated_compare_target_axis", "delegated_from": relative,
                "target_axis": list(TARGETS),
                "target_axis_lines_1based": [helper.count("\n", 0, helper_axis.start()) + 1,
                                              helper.count("\n", 0, helper_axis.end()) + 1]})
    require(bool(bindings), "missing Rust source binding: " + path)
    return bindings


def catalog_rows(sources: Sources, directory: str) -> list[tuple[str, int, dict[str, Any]]]:
    root = sources.repository / directory
    rows = []
    # Module paths are finite, source bound, and validated by the catalog count.
    header = directory + ".rs"
    header_text = sources.read(header).decode()
    expected = re.search(r'pub const CASE_COUNT: usize = (\d+);', header_text)
    for file in sorted(root.rglob("part_*.rs")):
        path = file.relative_to(sources.repository).as_posix()
        for index, row in enumerate(sources.json(path, "CASES")):
            require(isinstance(row, dict), "catalog row must be an object")
            rows.append((path, index, row))
    require(expected is not None and len(rows) == int(expected[1]), "catalog count differs: " + directory)
    return rows


def resolve_reference(sources: Sources, reference: dict[str, Any]) -> Any:
    """Resolve a standard JSON pointer and verify its source and value hash."""
    path = relative_path(reference["source_id"])
    raw = sources.read(path, reference["sha256"])
    text = reference["json_pointer"]
    if text == "" and "value_sha256" not in reference and not reference.get("constant") and not path.endswith(".json"):
        return raw
    value = sources.json(path, reference.get("constant"))
    require(text == "" or text.startswith("/"), "invalid reference JSON pointer")
    for raw in text.split("/")[1:] if text else []:
        require(re.search(r"~(?:[^01]|$)", raw) is None, "invalid JSON pointer escape")
        token = raw.replace("~1", "/").replace("~0", "~")
        if isinstance(value, list):
            require(bool(re.fullmatch(r"0|[1-9][0-9]*", token)), "invalid array pointer index")
            key: Any = int(token)
        else:
            key = token
        value = select(value, [key])
    if "value_sha256" in reference:
        require(digest(encode(value)) == reference["value_sha256"], "reference value hash differs")
    return value


def identity(row: Any) -> dict[str, Any]:
    if not isinstance(row, dict):
        return {}
    keys = ("id", "name", "case", "filename", "source_sha256", "python", "python_target", "target", "original_id")
    result = {}
    for key in keys:
        value = row.get(key)
        if isinstance(value, (str, int, bool)):
            if isinstance(value, str) and ("/Users/" in value or "/private/" in value or value.startswith("/")):
                result[key + "_value_sha256"] = digest(encode(value))
            else:
                result[key] = value
    return result


def input_bindings(sources: Sources, path: str, parts: list[Any], row: Any,
                   constant: str | None = None) -> list[dict[str, Any]]:
    """Bind source bytes and file maps without copying original private paths."""
    if not isinstance(row, dict):
        return []
    result = []
    for key in ("source", "source_utf8", "source_hex", "source_bytes_hex"):
        value = row.get(key)
        if not isinstance(value, str):
            continue
        raw = bytes.fromhex(value) if key.endswith("hex") else value.encode()
        if key in ("source", "source_utf8", "source_hex", "source_bytes_hex"):
            declared = row.get("source_sha256", row.get("sha256"))
            if isinstance(declared, str):
                require(digest(raw) == declared, "source bytes/hash differ: " + path)
        binding = {"pointer": sources.ref(path, parts + [key], constant),
                   "source_bytes_sha256": digest(raw), "bytes": len(raw)}
        if binding not in result:
            result.append(binding)
    for key, hash_key in (("files", "source_sha256"), ("local", "local_sha256"), ("packages", "package_sha256")):
        values = row.get(key)
        hashes = row.get(hash_key)
        if isinstance(values, dict):
            for name, value in values.items():
                if not isinstance(value, str):
                    continue
                actual = digest(value.encode())
                if isinstance(hashes, dict) and name in hashes:
                    require(hashes[name] == actual, "file bytes/hash differ: " + path)
                item = {"pointer": sources.ref(path, parts + [key, name], constant),
                        "source_bytes_sha256": actual, "bytes": len(value.encode())}
                if not name.startswith("/"):
                    item["filename"] = name
                result.append(item)
    return result


def auxiliary_input_bindings(sources: Sources, path: str, parts: list[Any], row: Any,
                             constant: str | None = None) -> list[dict[str, Any]]:
    """Retain byte files, symlink identity, and separate source/oracle joins."""
    if not isinstance(row, dict):
        return []
    result = []
    files = row.get("files")
    file_items = list(enumerate(files)) if isinstance(files, list) else list(files.items()) if isinstance(files, dict) else []
    for name, value in file_items:
        if not isinstance(value, dict):
            continue
        key = "bytes" if isinstance(value.get("bytes"), list) else "text" if isinstance(value.get("text"), str) else None
        if key is None:
            continue
        raw = bytes(value[key]) if key == "bytes" else value[key].encode()
        require(value["sha256"] == digest(raw), "embedded file bytes/hash differ: " + path)
        result.append({"pointer": sources.value_ref(path, parts + ["files", name, key], constant),
                       "source_bytes_sha256": digest(raw), "bytes": len(raw)})
    entries = row.get("entries", {})
    if isinstance(entries, dict):
        for name, value in entries.items():
            require(value["kind"] in ("regular", "symlink"), "unknown filesystem entry kind")
            key = "bytes_hex" if value["kind"] == "regular" else "link_target"
            raw = bytes.fromhex(value[key]) if key == "bytes_hex" else value[key].encode()
            require(value["sha256"] == digest(raw) and value["bytes"] == len(raw), "filesystem entry identity differs")
            result.append({"pointer": sources.value_ref(path, parts + ["entries", name, key], constant),
                           "entry_kind": value["kind"], "bytes": len(raw),
                           "source_bytes_sha256" if value["kind"] == "regular" else "link_target_bytes_sha256": digest(raw),
                           "symlink_target_is_dereferenced_content": False})
    symlinks = row.get("symlinks", {})
    if isinstance(symlinks, dict):
        for name, target in symlinks.items():
            require(isinstance(target, str), "invalid symlink target metadata")
            raw = target.encode()
            result.append({"pointer": sources.value_ref(path, parts + ["symlinks", name], constant),
                           "entry_kind": "symlink", "bytes": len(raw),
                           "link_target_bytes_sha256": digest(raw),
                           "symlink_target_is_dereferenced_content": False})
    if path.endswith(("/set_comprehensions_oracles.json", "/filtered_set_comprehensions_oracles.json")):
        source_path = path.replace("_oracles.json", "_sources.json")
        matches = [(i, value) for i, value in enumerate(sources.json(source_path)["inputs"]) if value["name"] == row["case"]]
        require(len(matches) == 1, "missing oracle/source input binding")
        index, value = matches[0]
        raw = value["source"].encode()
        require(digest(raw) == row["source_sha256"], "oracle/source bytes differ")
        result.append({"pointer": sources.value_ref(source_path, ["inputs", index, "source"]),
                       "source_bytes_sha256": digest(raw), "bytes": len(raw)})
    if path.endswith("/tuple_binding_oracles.json"):
        source_path = INFRA + "mypy/tests/tuple_binding.py"
        raw = sources.read(source_path, row["source_sha256"])
        result.append({"pointer": sources.ref(source_path), "source_bytes_sha256": digest(raw), "bytes": len(raw)})
    if path.endswith("/safety_logs.json"):
        source_path = INFRA + "black/tests/safety.json"
        matches = [(i, value) for i, value in enumerate(sources.json(source_path)) if value["source_sha256"] == row["source_sha256"]]
        require(bool(matches), "missing Black safety log source binding")
        for index, value in matches:
            raw = value["source"].encode()
            require(digest(raw) == row["source_sha256"], "Black safety source bytes differ")
            result.append({"pointer": sources.value_ref(source_path, [index, "source"]),
                           "source_bytes_sha256": digest(raw), "bytes": len(raw)})
    return result


def normalize_target(value: Any) -> str:
    text = str(value)
    if re.fullmatch(r"py31[234]", text):
        text = "3." + text[3:]
    require(text in TARGETS, "unknown declared Python target")
    return text


def target_sections(sources: Sources, path: str, parts: list[Any], row: Any,
                    owners: list[dict[str, Any]], constant: str | None = None) -> dict[str, Any]:
    """State the source of the target axis. Never infer it from length alone."""
    if not isinstance(row, dict):
        return {"kind": "unknown", "sections": []}
    for key in ("python", "target", "python_target"):
        if key in row:
            value = normalize_target(row[key])
            return {"kind": "row_target", "sections": [{"target": value,
                    "target_pointer": sources.value_ref(path, parts + [key], constant),
                    "record_pointer": sources.value_ref(path, parts, constant)}]}
    nested = row.get("targets")
    if isinstance(nested, list) and nested and all(isinstance(v, dict) and "target" in v for v in nested):
        return {"kind": "nested_targets", "sections": [
            {"target": normalize_target(value["target"]), "record_pointer": sources.value_ref(path, parts + ["targets", index], constant)}
            for index, value in enumerate(nested)]}
    expected = row.get("expected")
    if isinstance(expected, list) and len(expected) == 3:
        if all(isinstance(v, dict) and str(v.get("target", v.get("python"))) in TARGETS for v in expected):
            targets = [str(v.get("target", v.get("python"))) for v in expected]
            require(targets == list(TARGETS), "named expected target order differs: " + path)
            axis_kind = "named_expected_targets"
        elif any(owner.get("target_axis") == list(TARGETS) for owner in owners):
            targets = list(TARGETS)
            axis_kind = "owner_indexed_expected"
        else:
            return {"kind": "target_provenance_unknown", "sections": [],
                    "stored_reference_value_slots": [sources.value_ref(path, parts + ["expected", i], constant) for i in range(3)]}
        return {"kind": axis_kind, "sections": [
            {"target": target, "expected_pointer": sources.value_ref(path, parts + ["expected", i], constant)}
            for i, target in enumerate(targets)]}
    return {"kind": "unknown", "sections": []}


def tier_a(sources: Sources) -> dict[str, Any]:
    sources.read(PROFILE, "59cd5c03b0ab25ba7f994bcd81fece95dd84c613f1e0ca4dc947573b7bcae2e7")
    profile = sources.json(PROFILE)
    require(profile["python_versions"] == list(TARGETS), "profile target axis differs")
    units = []

    def add(tool: str, kind: str, name: str, parts: list[Any]) -> None:
        value = select(profile, parts)
        unit = {"unit_id": tool + ":" + kind + ":" + name, "tool": tool,
                "kind": kind, "declaration": sources.value_ref(PROFILE, parts), "literal": value,
                "tool_version": profile["versions"][tool],
                "profile_sha256": sources.facts[PROFILE]["sha256"],
                "target_roles": [{"target": target, "status": "unknown",
                    "roles": {role: {"status": "unknown", "witnesses": []} for role in ROLES}}
                    for target in TARGETS], "accepted": False}
        units.append(unit)

    for tool in ("ruff", "pylint"):
        for i, rule in enumerate(profile[tool]["rules"]):
            add(tool, "rule", rule["code"], [tool, "rules", i])
    for i, flag in enumerate(profile["mypy"]["strict_flags"]):
        add("mypy", "strict-flag", flag, ["mypy", "strict_flags", i])
    for tool, keys in (("ruff", ("preview", "safe_fixes_only", "select")),
                       ("isort", ("profile",)), ("black", tuple(profile["black"])),
                       ("pydantic", tuple(profile["pydantic"]))):
        for key in keys:
            add(tool, "setting", key, [tool, key])
    for key in profile["isort"]["settings"]:
        add("isort", "setting", key, ["isort", "settings", key])
    by_id = {u["unit_id"]: u for u in units}
    # Preserve Ruff's literal hit/suppression meanings. These are observed
    # declarations, not new role acceptance and not a dedicated interaction case.
    ruff = INFRA + "ruff/tests/catalog.rs"
    metadata = sources.json(ruff, "METADATA")
    ruff_rows = catalog_rows(sources, INFRA + "ruff/tests/catalog")
    ruff_cases = {row["path"]: (path, index, row) for path, index, row in ruff_rows}
    require(len(ruff_cases) == len(ruff_rows), "duplicate Ruff catalog path")
    declarations = []
    for i, version in enumerate(metadata["versions"]):
        for code, value in version["matrix"].items():
            require("ruff:rule:" + code in by_id, "Ruff matrix rule is outside profile")
            record = {"unit_id": "ruff:rule:" + code, "target": version["python"],
                      "pointer": sources.value_ref(ruff, ["versions", i, "matrix", code], "METADATA"),
                      "literal": value, "author_role_terms": {"negative": "diagnostic_hit", "positive": "suppression"},
                      "dedicated_interaction_binding": "unknown", "accepted_role_witness": False}
            case_bindings = []
            for term in ("negative", "positive"):
                name = value.get(term)
                if name is None:
                    continue
                require(name in ruff_cases, "missing Ruff declared case binding")
                case_path, case_index, case = ruff_cases[name]
                target_index = TARGETS.index(version["python"])
                case_bindings.append({"author_role_term": term, "path": name,
                    "source_bytes_sha256": case["source_sha256"],
                    "case_pointer": sources.value_ref(case_path, [case_index], "CASES"),
                    "expected_pointer": sources.value_ref(case_path, [case_index, "expected", target_index], "CASES"),
                    "accepted_role_witness": False})
            record["case_bindings"] = case_bindings
            declarations.append(record)
    guards_path = "crates/mamba-quality/assets/pylint-rule-guards.json"
    guards = sources.json(guards_path)
    guard_records = []
    for i, version in enumerate(guards):
        target = ".".join(str(n) for n in version["python"][:2])
        require(target in TARGETS, "Pylint guard target differs")
        for code, value in version["rules"].items():
            guard_records.append({"unit_id": "pylint:rule:" + code, "target": target,
                "pointer": sources.value_ref(guards_path, [i, "rules", code]), "literal": value,
                "defaults_pointer": sources.value_ref(guards_path, [i, "defaults"]),
                "flags_pointer": sources.value_ref(guards_path, [i, "flags"]), "guard_is_semantic_role": False})
    registry_path = "crates/mamba-quality/assets/pylint-message-registry.json"
    messages = sources.json(registry_path)
    inventory_path = "docs/operations/python-quality/q0-44-reference-inventory.json"
    inventory = sources.json(inventory_path)
    observed_outside = inventory["per_tool_counts"]["pylint"]["observed_outside_enabled_codes"]
    scoped_codes = {u["unit_id"].removeprefix("pylint:rule:") for u in units if u["tool"] == "pylint"} | set(observed_outside)
    message_records = [{"rule": m["code"], "enabled_by_profile": "pylint:rule:" + m["code"] in by_id,
                        "pointer": sources.value_ref(registry_path, ["messages", i]), "literal": m}
                       for i, m in enumerate(messages["messages"]) if m["code"] in scoped_codes]
    shared = [{"unit_id": "shared:" + key, "kind": "shared_constraint_or_identity",
               "literal": profile[key], "pointer": sources.value_ref(PROFILE, [key])}
              for key in ("line_width", "default_python", "python_versions", "schema", "versions", "standard_library")]
    return {"meaning": "Heterogeneous declarations. These are not independent semantic behaviors.",
            "counts": {"profile_declarations": len(units), "ruff_rules": len(profile["ruff"]["rules"]),
                "pylint_rules": len(profile["pylint"]["rules"]), "mypy_strict_flags": len(profile["mypy"]["strict_flags"]),
                "other_profile_constraints": 17, "shared_constraints_or_identities": len(shared)},
            "units": units, "shared_constraints": shared,
            "ruff_existing_target_declarations": declarations, "pylint_guard_declarations": guard_records,
            "pylint_message_declarations": message_records,
            "pylint_observed_information_codes_outside_enabled_set": {"literal": observed_outside,
                "pointer": sources.value_ref(inventory_path, ["per_tool_counts", "pylint", "observed_outside_enabled_codes"])},
            "profile_loader_source": sources.ref("crates/mamba-quality/src/domain/profile.rs"), "full_semantic_role_coverage_accepted": False}
SELECTOR_AXIS_ID = "mypy:catalog-selector:caller-target-axis"
SELECTOR_CALLER = INFRA + "mypy/tests.rs"
SELECTOR_WRAPPER = INFRA + "mypy/tests/builtin_values_regressions.rs"
SELECTOR_MANIFEST = INFRA + "mypy/tests/builtin_values_regressions.json"


def static_function_scope(text: str, name: str) -> tuple[str, int]:
    """Select one named Rust function through its next same-indent function."""
    starts = list(re.finditer(r"(?m)^(?P<indent>[ \t]*)(?:pub\([^)]*\)\s+)?fn\s+" + re.escape(name) + r"\s*\(", text))
    require(len(starts) == 1, "missing or ambiguous selector source function: " + name)
    start = starts[0]
    next_function = re.search(r"(?m)^" + re.escape(start["indent"]) + r"(?:pub\([^)]*\)\s+)?fn\s+", text[start.end():])
    end = start.end() + next_function.start() if next_function else len(text)
    return text[start.start():end], start.start()


def mypy_selector_inherited_axis(sources: Sources) -> dict[str, Any]:
    """Bind selector positions to the caller and the receiving wrapper.

    Three expected values do not establish an axis. The caller must enumerate
    the declared Python versions, index by that position, and forward the same
    version and position to the wrapper. The wrapper must retain that index.
    """
    caller = sources.read(SELECTOR_CALLER).decode()
    wrapper = sources.read(SELECTOR_WRAPPER).decode()
    body, offset = static_function_scope(caller, "pinned_original_corpus")
    receiver, receiver_offset = static_function_scope(wrapper, "record")

    def one(text: str, pattern: str, reason: str) -> re.Match[str]:
        matches = list(re.finditer(pattern, text, re.S))
        require(len(matches) == 1, reason)
        return matches[0]

    def span(path: str, text: str, match: re.Match[str], start: int) -> dict[str, Any]:
        begin, end = start + match.start(), start + match.end()
        return {"source_id": path, "sha256": sources.facts[path]["sha256"],
                "lines_1based": [text.count("\n", 0, begin) + 1, text.count("\n", 0, end) + 1],
                "source_fragment_sha256": digest(text[begin:end].encode())}

    construction = one(body, r"let\s+mut\s+builtin\s*=\s*builtin_values_regressions::BuiltinRegressions::new\(\)\s*;",
                       "mypy selector caller/wrapper construction differs")
    selection = one(body, r'let\s+selected\s*=\s*builtin\.select\(case\["name"\]\.as_str\(\)\.unwrap\(\)\)\s*;',
                    "mypy selector caller selection differs")
    axis = one(body, r"for\s*\(\s*target\s*,\s*version\s*\)\s*in\s*\[(?P<versions>[^\]]+)\]\s*\.into_iter\(\)\s*\.enumerate\(\)\s*\{",
               "mypy inherited selector target enumeration differs")
    versions = [value.strip() for value in axis["versions"].split(",") if value.strip()]
    require(versions == ["PythonVersion::Py312", "PythonVersion::Py313", "PythonVersion::Py314"],
            "mypy inherited selector target axis/order differs")
    assignment = one(body, r"workspace\.projects\[0\]\.python_version\s*=\s*version\s*;",
                     "mypy selector version assignment differs")
    expected = one(body, r'let\s+expected\s*=\s*&case\["expected"\]\[target\]\s*;',
                   "mypy selector expected target indexing differs")
    forwarding = one(body, r"if\s+selected\s*\{\s*builtin\.record\(\s*&case\s*,\s*version\s*,\s*target\s*,\s*&report\s*,\s*&actual\s*\)\s*;\s*\}",
                     "mypy selector target forwarding differs")
    require(construction.start() < selection.start() < axis.start() < assignment.start() < expected.start() < forwarding.start(),
            "mypy selector caller operation order differs")
    signature = one(receiver, r"fn\s+record\s*\((?P<parameters>.*?)\)\s*\{",
                    "mypy selector wrapper signature differs")
    parameters = [re.sub(r"\s+", "", value) for value in signature["parameters"].split(",") if value.strip()]
    require(parameters == ["&mutself", "case:&Value", "version:PythonVersion", "target:usize", "report:&ToolReport", "actual:&Value"],
            "mypy selector wrapper target parameter differs")
    receiver_expected = one(receiver, r'let\s+expected\s*=\s*&case\["expected"\]\[target\]\s*;',
                            "mypy selector wrapper expected target indexing differs")
    return {"relation_id": SELECTOR_AXIS_ID, "kind": "inherited_caller_enumeration",
            "target_axis": list(TARGETS), "target_indices": [0, 1, 2],
            "selector_manifest": sources.ref(SELECTOR_MANIFEST),
            "caller_source": sources.ref(SELECTOR_CALLER),
            "wrapper_source": sources.ref(SELECTOR_WRAPPER),
            "wrapper_construction": span(SELECTOR_CALLER, caller, construction, offset),
            "selector_selection": span(SELECTOR_CALLER, caller, selection, offset),
            "caller_target_enumeration": span(SELECTOR_CALLER, caller, axis, offset),
            "caller_version_assignment": span(SELECTOR_CALLER, caller, assignment, offset),
            "caller_expected_indexing": span(SELECTOR_CALLER, caller, expected, offset),
            "caller_target_forwarding": span(SELECTOR_CALLER, caller, forwarding, offset),
            "wrapper_target_parameter": span(SELECTOR_WRAPPER, wrapper, signature, receiver_offset),
            "wrapper_expected_indexing": span(SELECTOR_WRAPPER, wrapper, receiver_expected, receiver_offset),
            "accepted_role_witness": False, "native_product_status": "not_run"}


def tier_b(sources: Sources) -> dict[str, Any]:
    selector_axis = mypy_selector_inherited_axis(sources)
    mypy_directory = INFRA + "mypy/tests/catalog"
    main = catalog_rows(sources, mypy_directory)
    by_name: dict[str, tuple[str, int, dict[str, Any]]] = {}
    for path, index, row in main:
        name = row["name"]
        require(name not in by_name, "duplicate mypy catalog name")
        by_name[name] = (path, index, row)
    manifests = []
    originals = []
    whole_sources = []
    for spec_index, spec in enumerate(MANIFEST_SPECS):
        path = spec["path"]
        sources.read(path, spec["sha256"])
        constant = "CASES" if path.endswith(".rs") else None
        data = sources.json(path, constant)
        owners = owner_bindings(sources, path)
        container = spec["container"]
        selected = select(data, container) if container is not None else None
        kind = ("no_record_container" if selected is None else
                "legacy_string_character_entries" if isinstance(selected, str) else
                "object_mapping_records" if isinstance(selected, dict) else "array_records")
        manifest_id = spec["tool"] + ":ancillary:" + path
        manifest = {"unit_id": manifest_id, "tool": spec["tool"],
                    "source": sources.ref(path, constant=constant),
                    "container_pointer": pointer(container) if container is not None else None,
                    "container_kind": kind, "owning_rust_bindings": owners,
                    "legacy_record_entries": len(selected) if selected is not None else 0,
                    "declared_scalar_units": {k: v for k, v in data.items()
                        if isinstance(v, int) and not isinstance(v, bool) and
                        any(word in k for word in ("count", "cells", "cases", "sources"))}
                        if isinstance(data, dict) else {}, **unrun()}
        if path == SELECTOR_MANIFEST:
            manifest["inherited_target_axis_id"] = SELECTOR_AXIS_ID
        manifests.append(manifest)
        if isinstance(selected, str):
            require(data.get("source_sha256", digest(selected.encode())) == digest(selected.encode()), "whole scalar source identity differs")
            whole_sources.append({"unit_id": manifest_id + ":whole-source",
                "pointer": sources.value_ref(path, container), "bytes": len(selected.encode()),
                "source_bytes_sha256": digest(selected.encode()), "characters": len(selected),
                "target_sections": [{"target": target, "source_pointer": sources.value_ref(path, container)}
                                    for target in TARGETS] if any(o.get("target_axis") == list(TARGETS) for o in owners) else [],
                "legacy_character_entries_are_cases": False, **unrun()})
        iterable = selected.items() if isinstance(selected, dict) else enumerate(selected or [])
        for key, row in iterable:
            parts = container + [key]
            # Strings have no JSON-pointer child components. Keep the whole
            # scalar pointer and a separate character offset for compatibility.
            ref = sources.value_ref(path, container if isinstance(selected, str) else parts, constant)
            record = {"unit_id": manifest_id + ":record:" + str(key), "manifest_id": manifest_id,
                      "tool": spec["tool"], "legacy_record_entry_index": key,
                      "record_pointer": ref, "record_sha256": digest(encode(row)),
                      "identity": identity(row), "semantic_role_status": "unknown", **unrun()}
            if isinstance(selected, str):
                record.update({"unit_kind": "legacy_source_character_entry", "string_character_index": key,
                               "binding": {"kind": "character_is_not_case", "sections": []}})
            elif isinstance(row, str):
                require(row in by_name, "missing catalog selector binding: " + row)
                case_path, case_index, case = by_name[row]
                record.update({"unit_kind": "existing_catalog_selector", "selected_name": row,
                    "selected_case_pointer": sources.value_ref(case_path, [case_index], "CASES"),
                    "selected_case_identity": identity(case),
                    "binding": {"kind": "existing_catalog_selector", "inherited_target_axis_id": SELECTOR_AXIS_ID, "sections": [
                        {"target": target, "target_index": i, "expected_pointer": sources.value_ref(case_path, [case_index, "expected", i], "CASES")}
                        for i, target in enumerate(TARGETS)]},
                    "input_bindings": input_bindings(sources, case_path, [case_index], case, "CASES")})
            else:
                record.update({"unit_kind": "original_record_entry",
                    "input_bindings": input_bindings(sources, path, parts, row, constant) + auxiliary_input_bindings(sources, path, parts, row, constant),
                    "input_boundary_bindings": {key: sources.value_ref(path, parts + [key], constant) for key in ("inputs", "files", "directories", "entries", "symlinks") if isinstance(row, dict) and key in row},
                    "binding": target_sections(sources, path, parts, row, owners, constant)})
                if path.endswith(("/set_comprehensions_sources.json", "/filtered_set_comprehensions_sources.json")):
                    oracle_path = path.replace("_sources.json", "_oracles.json")
                    oracles = sources.json(oracle_path)
                    matches = [(i, o) for i, o in enumerate(oracles) if o["case"] == row["name"]]
                    require(len(matches) == 3, "missing source/oracle join")
                    require(all(o["source_sha256"] == digest(row["source"].encode()) for _, o in matches), "source/oracle identity differs")
                    record["binding"] = {"kind": "source_oracle_join", "sections": [
                        {"target": o["target"], "oracle_pointer": sources.value_ref(oracle_path, [i])}
                        for i, o in matches]}
            originals.append(record)

    projects_path = INFRA + "mypy/tests/projects/catalog"
    project_owner = INFRA + "mypy/tests/projects.rs"
    project_text = sources.read(project_owner).decode()
    require(bool(re.search(r'PythonVersion::Py312\s*,\s*PythonVersion::Py313\s*,\s*PythonVersion::Py314', project_text)), "project target axis differs")
    projects = []
    for path, index, row in catalog_rows(sources, projects_path):
        require(len(row["expected"]) == 3, "project expected axis differs")
        projects.append({"unit_id": "mypy:project:" + row["name"],
                         "record_pointer": sources.value_ref(path, [index], "CASES"),
                         "identity": identity(row), "input_bindings": input_bindings(sources, path, [index], row, "CASES"),
                         "target_sections": [{"target": target, "expected_pointer": sources.value_ref(path, [index, "expected", i], "CASES")}
                                             for i, target in enumerate(TARGETS)],
                         "target_axis_source": project_owner, **unrun()})
    interactions_path = INFRA + "ruff/tests/interactions.json"
    interactions = sources.json(interactions_path)
    ruff_fixture = {"unit_id": "ruff:separate-interactions", "kind": "separate_fixture_contract",
                    "source": sources.ref(interactions_path),
                    "sources": [{"name": name, "pointer": sources.value_ref(interactions_path, ["sources", name])}
                                for name in interactions["sources"]],
                    "rules": [{"code": code, "pointer": sources.value_ref(interactions_path, ["rules", code]), "literal": value}
                              for code, value in interactions["rules"].items()],
                    "version_sections": [{"pointer": sources.value_ref(interactions_path, ["versions", i])}
                                         for i, _ in enumerate(interactions["versions"])],
                    "missing_file_boundary": sources.value_ref(interactions_path, ["missing_file_boundary_kept_separate"]), **unrun()}
    variants_path = INFRA + "pylint/tests/variants.json"
    variants_data = sources.json(variants_path)
    variants = {"unit_id": "pylint:comparator-variants", "kind": "comparator_variant_metadata",
                "source": sources.ref(variants_path), "variants": [sources.value_ref(variants_path, ["cases", i]) for i, _ in enumerate(variants_data["cases"])],
                "runtime_case_count_claimed": False, **unrun()}
    safety_path = INFRA + "black/tests/safety.json"
    safety_log_path = INFRA + "black/tests/safety_logs.json"
    safety_data = sources.json(safety_path)
    safety_logs = sources.json(safety_log_path)
    safety = {"unit_id": "black:safety-API-recipe", "kind": "separate_safety_API_recipe_contract",
              "sources": [sources.value_ref(safety_path, [i]) for i, _ in enumerate(safety_data)],
              "safety_cells": [sources.value_ref(safety_log_path, ["cells", i]) for i, _ in enumerate(safety_logs["cells"])],
              "original_exact_worker_input_recipe": "unknown", "recipe_missing_is_not_success": True,
              "ordinary_formatter_case_is_safety_API_proof": False, **unrun()}
    config_path = INFRA + "mypy/tests/pydantic_config.json"
    config = sources.json(config_path)
    config_by_name = {r["name"]: i for i, r in enumerate(config)}
    entrypoints_path = INFRA + "mypy/tests/pydantic_config_entrypoints.json"
    entrypoint_data = sources.json(entrypoints_path)
    entrypoints = []
    for name, value in entrypoint_data.items():
        require(name in config_by_name, "missing Pydantic config entrypoint binding")
        entrypoints.append({"name": name, "entrypoint_metadata": sources.value_ref(entrypoints_path, [name]),
                            "config_project": sources.value_ref(config_path, [config_by_name[name]]), "literal": value})
    config_unit = {"unit_id": "pydantic:config-projects", "kind": "configuration_project_contract",
                   "shared_source_consumers": ["mypy", "pydantic"], "source": sources.ref(config_path),
                   "projects": [{"name": row["name"], "pointer": sources.value_ref(config_path, [i]),
                                "file_count": len(row["files"]), "target_sections": [
                                    {"target": target, "expected_pointer": sources.value_ref(config_path, [i, "expected", t])}
                                    for t, target in enumerate(TARGETS)]} for i, row in enumerate(config)],
                   "entrypoint_metadata": entrypoints, **unrun()}
    construct_path = INFRA + "mypy/tests/pydantic_construct.rs"
    construct_text = sources.read(construct_path).decode()
    selection = re.search(r'let selected = \[(.*?)\];', construct_text, re.S)
    require(selection is not None, "missing construct selector")
    selectors = re.findall(r'"([^"]+)"', selection[1])
    constructs = []
    for i, name in enumerate(selectors):
        require(name in by_name, "missing construct catalog binding")
        path, index, row = by_name[name]
        constructs.append({"unit_id": "pydantic:construct-selector:" + name,
                           "selector_source": construct_path, "selected_array_index": i,
                           "record_pointer": sources.value_ref(path, [index], "CASES"), "identity": identity(row),
                           "target_sections": [{"target": target, "expected_pointer": sources.value_ref(path, [index, "expected", t], "CASES")}
                                               for t, target in enumerate(TARGETS)],
                           "runtime_asset_case_claimed": False, **unrun()})
    return {"meaning": "Named contracts retain their own units and unrun gates. Overlap is not execution proof.",
            "counts": {"manifest_entries": len(manifests), "unique_manifest_source_files": len({s["path"] for s in MANIFEST_SPECS}),
                       "legacy_original_record_entries": len(originals),
                       "legacy_source_character_entries": sum(r["unit_kind"] == "legacy_source_character_entry" for r in originals),
                       "legacy_entry_binding_kinds": dict(sorted(Counter(r["binding"]["kind"] for r in originals).items())),
                       "whole_scalar_source_contracts": len(whole_sources),
                       "mypy_project_contracts": len(projects),
                       "mypy_project_source_files": sum(len(p["input_bindings"]) for p in projects),
                       "mypy_project_target_sections": sum(len(p["target_sections"]) for p in projects),
                       "black_safety_sources": len(safety["sources"]), "black_safety_cells": len(safety["safety_cells"]),
                       "pydantic_config_projects": len(config), "pydantic_config_source_files": sum(len(r["files"]) for r in config),
                       "pydantic_config_target_sections": len(config) * 3, "pydantic_config_entrypoint_records": len(entrypoints),
                       "pydantic_construct_selectors": len(constructs), "pydantic_construct_target_sections": len(constructs) * 3},
            "inherited_target_axes": [selector_axis],
            "manifests": manifests, "original_record_entries": originals, "whole_scalar_sources": whole_sources,
            "mypy_projects": projects, "ruff_interactions": ruff_fixture, "pylint_comparator_variants": variants,
            "black_safety": safety, "pydantic_config": config_unit, "pydantic_construct_selectors": constructs,
            "all_own_contract_reference_gates_accepted": False, "all_native_gates_accepted": False}


def tier_c(sources: Sources) -> dict[str, Any]:
    path = "docs/operations/python-quality/remaining-412-ledger.json"
    sources.read(path, "b8738b57ca23e007da415ded268ef313b33ae04e297003a1203ce204b8313871")
    ledger = sources.json(path)
    rows = []
    for i, row in enumerate(ledger["rows"]):
        # Exact unresolved strings remain at the immutable source pointer. Some
        # have private path text, so publish hashes instead of those strings.
        rows.append({"unit_id": "native-obligation:" + str(row["index"]),
            "pointer": sources.value_ref(path, ["rows", i]),
            "unresolved_condition_pointer": sources.value_ref(path, ["rows", i, "reason"]),
            "category": row["category"], "multiplicity": row["multiplicity"],
            "child_issue": row["child_issue"], "child_url": "https://github.com/faberline/mamba/issues/" + str(row["child_issue"]),
            "child_title": row["child_title"], "status": row["status"],
            "q0_implementation_requirement": False})
    return {"meaning": "Unresolved native obligations. This is not a count of bugs or a Q0 implementation gate.",
            "counts": {"obligation_records": len(rows), "children": len({r["child_issue"] for r in rows}),
                       "by_category": dict(sorted(Counter(r["category"] for r in rows).items()))},
            "rows": rows, "all_statuses": sorted({r["status"] for r in rows})}


def tier_d(sources: Sources) -> dict[str, Any]:
    sections = []
    for asset in ("field-types", "model-types", "model-methods", "model-config-field"):
        path = "crates/mamba-quality/assets/pydantic/" + asset + ".json"
        asset_pins = {
            "field-types": "3feeb55e6b8abf976d00d42bd7c5357a06264d171b49ec4f30528f4fe1c0c5ca",
            "model-types": "745e7173fbed74acf3924ff92d36c67a6ba271dae63951e6b666facd78cc71a9",
            "model-methods": "4d8b9edc282ed6e064aaf613b74adce81bed8f18b18cd9556bbe08f2c8db7fa9",
            "model-config-field": "f4f4a01379dba139751a74465618a158ab80952e7a680e5bc8c0ffedf0853a96",
        }
        sources.read(path, asset_pins[asset])
        data = sources.json(path)
        require(list(data["targets"]) == list(TARGETS), "Pydantic API target axis differs")
        for target in TARGETS:
            section = data["targets"][target]
            subunits: dict[str, Any] = {"top_level_section_keys": list(section)}
            if "aliases" in section:
                subunits["aliases"] = len(section["aliases"])
            if asset == "field-types":
                subunits["Field_overloads"] = len(section["serialized_type"]["items"])
                subunits["declared_variants"] = len(section["variants"])
            if asset == "model-methods":
                subunits["BaseModel_methods"] = len(section["model_types"]["methods"])
            sections.append({"unit_id": "pydantic:api-asset:" + asset + ":" + target,
                "kind": "whole_metadata_asset_target_section", "target": target,
                "pointer": sources.value_ref(path, ["targets", target]), "schema": data["schema"],
                "metadata_subunits": subunits, "runtime_case_count_claimed": False,
                "accepted_reference_issue": 164, "current_builder_execution": "not_run",
                "accepted_reference_audit_source": sources.ref("docs/operations/python-quality/q0-164-pydantic-reference-audit.md"),
                "reference_binding_status": "accepted_whole_asset_identity_retained",
                "native_product_status": "not_run"})
    return {"meaning": "Whole API metadata sections and metadata subunits. These are not runtime cases.",
            "counts": {"assets": 4, "whole_metadata_target_sections": len(sections)}, "sections": sections}
def supplemental_intents(sources: Sources, units: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Join public authored packets as intent only, never as observations."""
    by_id = {u["unit_id"]: u for u in units}
    intents = []
    for tool in ("mypy", "pylint", "pydantic"):
        path = "tools/ci/q0_16_cases/" + tool + ".json"
        packet = sources.json(path)
        for index, case in enumerate(packet["cases"]):
            behavior = case["behavior"]
            unit_id = ("mypy:strict-flag:" + behavior if tool == "mypy" else
                       "pydantic:setting:" + behavior.removeprefix("pydantic.") if tool == "pydantic" else
                       "pylint:rule:" + behavior)
            require(unit_id in by_id, "supplemental behavior has no profile binding")
            require(case["role"] in ROLES, "unknown supplemental role")
            files = case["files"]
            inputs = [{"filename": name, "sha256": digest(text.encode()), "bytes": len(text.encode()),
                       "pointer": sources.value_ref(path, ["cases", index, "files", name])}
                      for name, text in files.items()]
            intents.append({"unit_id": unit_id, "author_role": case["role"],
                "case_id": case["id"], "case_pointer": sources.value_ref(path, ["cases", index]),
                "python_targets": case["python_targets"], "input_bindings": inputs,
                "intended_predicates": sources.value_ref(path, ["cases", index, "requires"]),
                "status": "authored_not_run", "accepted_role_witness": False})
        for index, disposition in enumerate(packet.get("dispositions", [])):
            unit_id = tool + ":rule:" + disposition["behavior"]
            require(unit_id in by_id, "supplemental disposition has no profile binding")
            intents.append({"unit_id": unit_id, "author_role": "disposition",
                "case_pointer": sources.value_ref(path, ["dispositions", index]),
                "python_targets": disposition["python_targets"],
                "status": "authored_source_annotation", "accepted_role_witness": False})
    return intents


def role_annotations(sources: Sources, paths: list[Path], units: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Optional explicit author bindings. No private path or acceptance is copied.

    Each file is a JSON list of {unit_id, target, role, source_id, json_pointer,
    constant?}. Source IDs and pointers are checked against repository bytes.
    External source filenames are intentionally absent from the public result.
    """
    by_id = {u["unit_id"] for u in units}
    annotations = []
    for file in paths:
        raw = file.read_bytes()
        data = json.loads(raw)
        require(isinstance(data, list) and bool(data), "empty role annotation input")
        for value in data:
            require(value["unit_id"] in by_id and value["target"] in TARGETS and value["role"] in ROLES,
                    "unbound role annotation")
            source_id = relative_path(value["source_id"])
            text = value["json_pointer"]
            require(text == "" or text.startswith("/"), "invalid annotation JSON pointer")
            obj = sources.json(source_id, value.get("constant"))
            parts: list[Any] = []
            for item in text.split("/")[1:] if text else []:
                require(re.search(r"~(?:[^01]|$)", item) is None, "invalid JSON pointer escape")
                item = item.replace("~1", "/").replace("~0", "~")
                if isinstance(obj, list):
                    require(bool(re.fullmatch(r"0|[1-9][0-9]*", item)), "invalid array pointer index")
                part = int(item) if isinstance(obj, list) else item
                obj = select(obj, [part])
                parts.append(part)
            annotations.append({"unit_id": value["unit_id"], "target": value["target"],
                "author_role": value["role"], "pointer": sources.value_ref(source_id, parts, value.get("constant")),
                "annotation_input_sha256": digest(raw), "status": "author_annotation_not_accepted",
                "accepted_role_witness": False})
    return annotations


def validate_registry(registry: dict[str, Any], sources: Sources | None = None) -> None:
    """Fail closed on a lost unit, a mixed denominator, or any pass inflation."""
    require(registry["schema"] == "mamba.q0.16.required-behavior-registry.v1", "registry schema differs")
    expected = {
        "A": {"profile_declarations": 587, "ruff_rules": 174, "pylint_rules": 383,
              "mypy_strict_flags": 13, "other_profile_constraints": 17, "shared_constraints_or_identities": 6},
        "B": {"manifest_entries": 119, "unique_manifest_source_files": 118,
              "legacy_original_record_entries": 7205, "legacy_source_character_entries": 353,
              "whole_scalar_source_contracts": 2, "mypy_project_contracts": 318,
              "mypy_project_source_files": 728, "mypy_project_target_sections": 954,
              "black_safety_sources": 58, "black_safety_cells": 174,
              "pydantic_config_projects": 257, "pydantic_config_source_files": 341,
              "pydantic_config_target_sections": 771, "pydantic_config_entrypoint_records": 7,
              "pydantic_construct_selectors": 3, "pydantic_construct_target_sections": 9},
        "C": {"obligation_records": 412, "children": 29},
        "D": {"assets": 4, "whole_metadata_target_sections": 12},
    }
    for tier, counts in expected.items():
        for unit, count in counts.items():
            require(count > 0 and registry["tiers"][tier]["counts"][unit] == count, "unit denominator differs: " + tier + "/" + unit)
    a, b, c, d = (registry["tiers"][key] for key in "ABCD")
    require(len(a["units"]) == 587 and len({u["unit_id"] for u in a["units"]}) == 587, "missing or duplicate profile declaration")
    require(len(a["shared_constraints"]) == 6, "missing shared identity")
    for unit in a["units"]:
        require(unit["accepted"] is False, "unknown profile declaration inflated to pass")
        require([t["target"] for t in unit["target_roles"]] == list(TARGETS), "missing profile target binding")
        for target in unit["target_roles"]:
            require(target["status"] == "unknown", "unknown profile target inflated to pass")
            require(set(target["roles"]) == set(ROLES), "missing role slot")
            require(all(r["status"] == "unknown" and r["witnesses"] == [] for r in target["roles"].values()), "unknown role inflated to pass")
    require(len(a["ruff_existing_target_declarations"]) == 522, "Ruff rule-target declaration count differs")
    require(len(a["pylint_guard_declarations"]) == 69, "Pylint guard count differs")
    require(len(a["pylint_message_declarations"]) == 389, "Pylint message count differs")
    require(len(b["manifests"]) == 119, "missing manifest binding")
    require(len({m["unit_id"] for m in b["manifests"]}) == 119, "duplicate manifest binding")
    manifest_ids = {m["unit_id"] for m in b["manifests"]}
    axes = b["inherited_target_axes"]
    require(len(axes) == 1 and axes[0]["relation_id"] == SELECTOR_AXIS_ID and
            axes[0]["target_axis"] == list(TARGETS) and axes[0]["target_indices"] == [0, 1, 2],
            "missing inherited selector target axis")
    selector_manifests = [m for m in b["manifests"] if m["source"]["source_id"] == SELECTOR_MANIFEST]
    require(len(selector_manifests) == 1 and selector_manifests[0]["inherited_target_axis_id"] == SELECTOR_AXIS_ID,
            "missing selector manifest/caller relation")
    if sources is not None:
        require(axes[0] == mypy_selector_inherited_axis(sources), "inherited selector source relation differs")
    require(len(b["original_record_entries"]) == 7205, "missing legacy record binding")
    for row in b["original_record_entries"]:
        require(row["manifest_id"] in manifest_ids, "orphan original record")
        require(bool(row["record_pointer"]) and bool(row["record_sha256"]), "missing record identity")
        if row["unit_kind"] == "legacy_source_character_entry":
            require(row["binding"]["kind"] == "character_is_not_case" and not row["binding"]["sections"], "character inflated to case")
        binding = row["binding"]
        kind = binding["kind"]
        if kind == "existing_catalog_selector":
            require(binding.get("inherited_target_axis_id") == SELECTOR_AXIS_ID and
                    row["manifest_id"] == selector_manifests[0]["unit_id"] and
                    [section.get("target_index") for section in binding["sections"]] == [0, 1, 2],
                    "missing selector inherited target binding")
            require(all(section["expected_pointer"]["json_pointer"] ==
                        row["selected_case_pointer"]["json_pointer"] + "/expected/" + str(section["target_index"])
                        for section in binding["sections"]), "selector expected target position differs")
        if kind == "row_target":
            require(len(binding["sections"]) == 1 and binding["sections"][0]["target"] in TARGETS, "ancillary row target differs")
            if sources is not None:
                section = binding["sections"][0]
                require(normalize_target(resolve_reference(sources, section["target_pointer"])) == section["target"], "ancillary target pointer differs")
        elif kind in ("existing_catalog_selector", "named_expected_targets", "nested_targets", "owner_indexed_expected", "source_oracle_join"):
            require([section["target"] for section in binding["sections"]] == list(TARGETS), "missing ancillary target section")
            if sources is not None and kind in ("named_expected_targets", "nested_targets", "source_oracle_join"):
                key = "expected_pointer" if kind == "named_expected_targets" else "record_pointer" if kind == "nested_targets" else "oracle_pointer"
                for section in binding["sections"]:
                    value = resolve_reference(sources, section[key])
                    require(normalize_target(value.get("target", value.get("python"))) == section["target"], "ancillary expected target pointer differs")
        elif kind == "target_provenance_unknown":
            require(row["record_pointer"]["source_id"].endswith("/width_data.rs") and not binding["sections"] and len(binding["stored_reference_value_slots"]) == 3, "unknown target boundary differs")
        else:
            require(kind == "character_is_not_case", "unknown ancillary binding kind")
    require(sum(m["legacy_record_entries"] for m in b["manifests"]) == 7205, "manifest/record count mismatch")
    require(len(b["mypy_projects"]) == 318 and sum(len(p["target_sections"]) for p in b["mypy_projects"]) == 954, "missing project target binding")
    require(len(b["ruff_interactions"]["sources"]) == 29 and len(b["ruff_interactions"]["rules"]) == 14, "Ruff separate boundary differs")
    require(len(b["pylint_comparator_variants"]["variants"]) == 29, "Pylint comparator boundary differs")
    require(len({row["unit_id"] for row in b["original_record_entries"]}) == 7205, "duplicate original record binding")
    require(Counter(row["tool"] for row in b["original_record_entries"]) ==
            {"black": 899, "isort": 294, "mypy": 4789, "pydantic": 257, "pylint": 966}, "original record consumer counts differ")
    require(sum(len(row["input_bindings"]) for row in b["mypy_projects"]) == 728, "missing project file binding")
    require(all([t["target"] for t in row["target_sections"]] == list(TARGETS) for row in b["mypy_projects"]), "missing project target section")
    require(len(b["black_safety"]["sources"]) == 58 and len(b["black_safety"]["safety_cells"]) == 174, "missing safety boundary binding")
    require(b["black_safety"]["original_exact_worker_input_recipe"] == "unknown", "unknown safety recipe inflated")
    require(len(b["pydantic_config"]["projects"]) == 257 and len(b["pydantic_config"]["entrypoint_metadata"]) == 7, "missing Pydantic config boundary binding")
    require(sum(row["file_count"] for row in b["pydantic_config"]["projects"]) == 341, "missing Pydantic config file binding")
    require(len(b["pydantic_construct_selectors"]) == 3 and sum(len(row["target_sections"]) for row in b["pydantic_construct_selectors"]) == 9, "missing construct selector binding")
    require(a["full_semantic_role_coverage_accepted"] is False and b["all_own_contract_reference_gates_accepted"] is False and b["all_native_gates_accepted"] is False, "tier acceptance inflated")
    require(len(c["rows"]) == 412 and len({row["unit_id"] for row in c["rows"]}) == 412 and all(row["status"] == "unresolved" for row in c["rows"]), "native obligation status changed")
    require(c["counts"]["by_category"] == {"capability": 4, "diagnosis_marker": 407, "global_summary": 1}, "native obligation categories differ")
    require(len(d["sections"]) == 12 and all(not row["runtime_case_count_claimed"] for row in d["sections"]), "metadata inflated to runtime cases")
    require({row["unit_id"] for row in d["sections"]} == {"pydantic:api-asset:" + asset + ":" + target for asset in ("field-types", "model-types", "model-methods", "model-config-field") for target in TARGETS}, "missing API asset target binding")
    for row in d["sections"]:
        if "Field_overloads" in row["metadata_subunits"]:
            require(row["metadata_subunits"]["Field_overloads"] == 6, "Field overload count differs")
        if "BaseModel_methods" in row["metadata_subunits"]:
            require(row["metadata_subunits"]["BaseModel_methods"] == 26, "BaseModel method count differs")
    require(all(value is False for value in registry["acceptance"].values()), "static registry cannot claim acceptance")
    known_sources = {source["source_id"]: source for source in registry["sources"]}
    require(bool(known_sources) and len(known_sources) == len(registry["sources"]), "missing or duplicate source facts")
    def walk(value: Any) -> None:
        if isinstance(value, dict):
            if "source_id" in value:
                source_id = relative_path(value["source_id"])
                require(source_id in known_sources, "missing source binding")
                if "sha256" in value:
                    require(value["sha256"] == known_sources[source_id]["sha256"], "source binding hash differs")
            if sources is not None and "source_id" in value and "json_pointer" in value:
                resolve_reference(sources, value)
            if "accepted_role_witness" in value:
                require(value["accepted_role_witness"] is False, "unknown/unrun role inflated to pass")
            if "native_product_status" in value:
                require(value["native_product_status"] == "not_run", "unrun native gate inflated to pass")
            if "reference_execution_status" in value:
                require(value["reference_execution_status"] == "not_run_as_own_contract", "unrun reference gate inflated to pass")
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)
    walk(registry)
    rendered = json.dumps(registry)
    require("/Users/" not in rendered and "/private/tmp/" not in rendered and "file://" not in rendered,
            "private path leaked into public registry")
    if sources:
        for source in registry["sources"]:
            raw = (sources.repository / source["path"]).read_bytes()
            require(digest(raw) == source["sha256"], "source changed during static build")


def build_registry(repository: Path, annotation_paths: list[Path] | None = None) -> dict[str, Any]:
    sources = Sources(repository)
    a = tier_a(sources)
    b = tier_b(sources)
    c = tier_c(sources)
    d = tier_d(sources)
    intents = supplemental_intents(sources, a["units"])
    annotations = role_annotations(sources, annotation_paths or [], a["units"] + a["shared_constraints"])
    registry = {"schema": "mamba.q0.16.required-behavior-registry.v1", "issue": 16,
        "state": "static_inventory_with_unrun_gates", "language_targets": list(TARGETS),
        "unit_policy": "Keep each denominator separate. Never sum overlapping tiers into a passed-case count.",
        "source_policy": "Repository source bytes and pointers establish identity. They do not establish execution.",
        "tiers": {"A": a, "B": b, "C": c, "D": d},
        "supplemental_author_intents": intents, "external_author_annotations": annotations,
        "acceptance": {"full_semantic_role_coverage": False, "all_own_contract_reference_gates": False,
                       "all_native_product_gates": False, "parent_16": False, "parent_15": False,
                       "full_reference_platform_SOM_release": False},
        "sources": [sources.facts[path] for path in sorted(sources.facts)]}
    validate_registry(registry, sources)
    return registry


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check", action="store_true", help="compare against the saved registry without writing")
    parser.add_argument("--role-annotations", action="append", type=Path, default=[])
    args = parser.parse_args()
    registry = build_registry(args.repository, args.role_annotations)
    output = args.output or args.repository / OUTPUT
    encoded = json.dumps(registry, ensure_ascii=True, separators=(",", ":")) + "\n"
    if args.check:
        require(output.read_text() == encoded, "saved registry differs; rebuild the static inventory")
    else:
        output.write_text(encoded)
    print(json.dumps({"schema": registry["schema"], "state": registry["state"],
                      "counts_by_tier": {k: v["counts"] for k, v in registry["tiers"].items()},
                      "accepted": False}, sort_keys=True))


if __name__ == "__main__":
    main()
