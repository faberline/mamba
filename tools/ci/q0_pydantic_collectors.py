"""Render independent original-API observers. This module uses stdlib only.

Rendered code runs only in the hosted reference environments. It keeps the
four original extraction boundaries and writes new custody data separately.
"""
from pathlib import Path


FAMILIES = ("field-types", "model-types", "model-methods", "model-config-field")
VERSIONS = {12: "3.12.11", 13: "3.13.5", 14: "3.14.0"}
CONFIG = ("[mypy]\nplugins = pydantic.mypy\n[pydantic-mypy]\ninit_typed = true\n"
          "init_forbid_extra = true\nwarn_required_dynamic_aliases = true\n")
STUBS = {
    "field-types": "from pydantic import Field as Field\n",
    "model-types": "from pydantic import BaseModel as BaseModel, ConfigDict as ConfigDict, Field as Field\n",
    "model-methods": "from pydantic import BaseModel as BaseModel, ConfigDict as ConfigDict, Field as Field\n",
    "model-config-field": "from pydantic import BaseModel as BaseModel, ConfigDict as ConfigDict\n",
}
METHODS = ("model_dump", "model_dump_json", "model_copy", "model_fields_set", "model_validate",
           "model_validate_json", "model_validate_strings", "model_extra", "model_json_schema",
           "model_parametrized_name", "model_post_init", "model_rebuild", "dict", "json", "parse_obj",
           "parse_raw", "parse_file", "from_orm", "construct", "copy", "schema", "schema_json",
           "validate", "update_forward_refs", "__copy__", "__deepcopy__")
LEGACY_METHODS = METHODS[12:]

_READERS = r'''
def _alias_reader(graph, allow_typed_dict):
    found = {}
    def add(value):
        if isinstance(value, list):
            for part in value:
                add(part)
        elif isinstance(value, dict):
            if value.get('.class') == 'TypeAliasType':
                identity = value['type_ref']
                if identity not in found:
                    module, symbol = identity.rsplit('.', 1)
                    node = graph[module].tree.names[symbol].node
                    target = (node.target if hasattr(node, 'target') else node.typeddict_type) if allow_typed_dict else node.target
                    serialized = target.serialize()
                    found[identity] = serialized
                    add(serialized)
            for part in value.values():
                add(part)
    return found, add

def _method_row(node, optional_flags=False):
    function = getattr(node, 'func', node)
    flags = lambda name: bool(getattr(function, name, False)) if optional_flags else bool(getattr(function, name))
    return {'type': node.type.serialize(), 'property': flags('is_property'),
            'class_method': flags('is_class'), 'static_method': flags('is_static')}

def _basic_models(graph, remember):
    base = graph['pydantic.main'].tree.names['BaseModel'].node
    config = graph['pydantic.config'].tree.names['ConfigDict'].node
    constructor = base.names['__init__'].node.type.serialize()
    dictionary = config.typeddict_type.serialize()
    remember(constructor)
    remember(dictionary)
    return {'base_model': {'fullname': base.fullname, 'init_type': constructor,
                           'bases': [item.serialize() for item in base.bases]},
            'config_dict': {'fullname': config.fullname, 'type': dictionary}}

def _initial_methods(graph, remember):
    base = graph['pydantic.main'].tree.names['BaseModel'].node
    rows = {}
    for name in _METHOD_NAMES:
        node = base.names[name].node
        rows[name] = _method_row(node)
        remember(node.type.serialize())
    schema = graph['pydantic.json_schema'].tree.names['GenerateJsonSchema'].node
    constructor = schema.names['__init__'].node.type.serialize()
    remember(constructor)
    output = {'fullname': base.fullname, 'methods': rows,
              'schema_generator': {'fullname': schema.fullname, 'init_type': constructor,
                                   'bases': [item.serialize() for item in schema.bases]}}
    properties = {}
    for name in ('model_fields', 'model_computed_fields'):
        node = base.names[name].node
        properties[name] = {'node_kind': type(node).__name__, 'type': node.type.serialize(),
                            'class_var': bool(getattr(node, 'is_classvar', False)),
                            'final': bool(getattr(node, 'is_final', False))}
        remember(node.type.serialize())
    descriptor = graph['pydantic._internal._utils'].tree.names['deprecated_instance_property'].node
    descriptor_rows = {}
    for name in ('__init__', '__get__'):
        node = descriptor.names[name].node
        descriptor_rows[name] = _method_row(node, True)
        remember(descriptor_rows[name]['type'])
    output['class_properties'] = properties
    output['class_property_descriptor'] = {'fullname': descriptor.fullname,
        'generic': [item.serialize() for item in descriptor.defn.type_vars],
        'bases': [item.serialize() for item in descriptor.bases], 'methods': descriptor_rows}
    return base, output

def _info_rows(graph, remember, variable_class, method_classes):
    output = {}
    for name in ('FieldInfo', 'ComputedFieldInfo'):
        info = graph['pydantic.fields'].tree.names[name].node
        members = {}
        for name_in_class, symbol in info.names.items():
            if name_in_class.startswith('_'):
                continue
            node = symbol.node
            if isinstance(node, variable_class):
                if node.type is None:
                    continue
                value = node.type.serialize()
                row = {'kind': 'field', 'type': value, 'class_var': bool(node.is_classvar),
                       'final': bool(node.is_final), 'initialized_in_class': bool(node.is_initialized_in_class)}
            elif isinstance(node, method_classes):
                if node.type is None:
                    continue
                function = getattr(node, 'func', node)
                value = node.type.with_unpacked_kwargs().serialize()
                row = {'kind': 'method', 'type': value, 'property': bool(getattr(function, 'is_property', False)),
                       'class_method': bool(getattr(function, 'is_class', False)),
                       'static_method': bool(getattr(function, 'is_static', False))}
            else:
                raise AssertionError((name, name_in_class, type(node).__name__))
            members[name_in_class] = row
            remember(value)
        output[name] = {'fullname': info.fullname, 'bases': [item.serialize() for item in info.bases],
                        'members': members, 'slots': sorted(info.slots) if info.slots is not None else None}
    for name in output:
        info = graph['pydantic.fields'].tree.names[name].node
        initializer = info.names['__init__'].node
        raw = initializer.type.serialize()
        remember(raw)
        output[name]['raw_initializer'] = raw
        unpacked = initializer.type.with_unpacked_kwargs().serialize()
        remember(unpacked)
        output[name]['initializer'] = unpacked
        output[name]['initializer_line'] = initializer.line
        output[name]['initializer_generated'] = info.names['__init__'].plugin_generated
        output[name]['final'] = bool(info.is_final)
        output[name]['exports'] = {module: bool(graph[module].tree.names.get(name) and graph[module].tree.names[name].module_public)
                                   for module in ('pydantic', 'pydantic.fields')}
    return output

def _remaining_methods(graph, base, output, remember):
    value = graph['pydantic'].tree.names['__getattr__'].node.type.serialize()
    output['root_getattr'] = value
    remember(value)
    protocols = {}
    for name, member in (('SupportsGt', '__gt__'), ('SupportsGe', '__ge__'), ('SupportsLt', '__lt__'), ('SupportsLe', '__le__')):
        info = graph['annotated_types'].tree.names[name].node
        value = info.names[member].node.type.serialize()
        remember(value)
        protocols[name] = {'fullname': info.fullname, 'protocol': bool(info.is_protocol),
                            'method': member, 'type': value}
    output['comparison_protocols'] = protocols
    scalar = {}
    for name in ('int', 'float', 'str', 'bytes'):
        info = graph['builtins'].tree.names[name].node
        scalar[name] = {}
        for member in ('__gt__', '__ge__', '__lt__', '__le__'):
            value = info.names[member].node.type.serialize()
            remember(value)
            scalar[name][member] = value
    output['scalar_comparisons'] = scalar
    value = graph['pydantic.fields'].tree.names['computed_field'].node.type.serialize()
    remember(value)
    output['computed_field'] = value
    cached = graph['functools'].tree.names['cached_property'].node
    rows = {}
    for name in ('__init__', '__get__', '__set__'):
        value = cached.names[name].node.type.serialize()
        remember(value)
        rows[name] = value
    output['cached_property'] = {'fullname': cached.fullname,
        'generic': [item.serialize() for item in cached.defn.type_vars],
        'bases': [item.serialize() for item in cached.bases], 'methods': rows}
    root = graph['pydantic.root_model'].tree.names['RootModel'].node
    rows = {}
    for name in ('__init__', 'model_construct', 'model_dump', '__copy__', '__deepcopy__'):
        rows[name] = _method_row(root.names[name].node, True)
        remember(rows[name]['type'])
    value = root.names['root'].node.type.serialize()
    remember(value)
    output['root_model'] = {'fullname': root.fullname,
        'generic': [item.serialize() for item in root.defn.type_vars],
        'bases': [item.serialize() for item in root.bases], 'root_type': value,
        'methods': rows, 'metadata': root.metadata['pydantic-mypy-metadata']}
    for name in _LEGACY_NAMES:
        output['methods'][name]['node_kind'] = type(base.names[name].node).__name__
'''

_METADATA = r'''
def _save_new(path, value):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')

def _file_fact(path):
    data = path.read_bytes()
    return {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}

def _path_category(path, roots, declared):
    path = path.resolve(strict=True)
    for name, root in roots.items():
        if path.is_relative_to(root):
            if name == 'base_stdlib' and any(part in ('site-packages', 'dist-packages') for part in path.relative_to(root).parts):
                return 'unexpected'
            return name
    return 'declared-input' if path in declared else 'unexpected'

def _object_alias(name, value, observed, loaded):
    owner_name = getattr(value, '__module__', None)
    qualname = getattr(value, '__qualname__', None)
    if type(owner_name) is not str or type(qualname) is not str:
        return None
    members = qualname.split('.')
    if name != owner_name + '.' + qualname or not all(part.isidentifier() for part in members):
        return None
    owner_fact = observed.get(owner_name, {})
    if owner_fact.get('category') not in ('venv', 'base_stdlib') or not owner_fact.get('path'):
        return None
    current = loaded.get(owner_name)
    try:
        # Read namespace dictionaries. getattr on typing.io/re members emits
        # deprecation warnings on3.12; dictionary access runs no descriptor.
        for part in members:
            namespace = vars(current)
            if part not in namespace:
                return None
            current = namespace[part]
    except TypeError:
        return None
    if current is not value:
        return None
    return {'path': None, 'category': 'object-alias', 'owner_module': owner_name,
            'owner_qualname': qualname, 'owner_members': members, 'identity_verified': True,
            'owner_path': owner_fact['path'], 'owner_bytes': owner_fact['bytes'],
            'owner_sha256': owner_fact['sha256']}

def _capture(result, work, profile, source, original, sidecars):
    # These metadata-only imports occur after every original value is derived.
    import os
    import sysconfig
    import importlib.machinery
    capture = Path(_CAPTURE)
    if capture.is_symlink():
        raise ValueError('collector capture must be a real fresh directory')
    capture.mkdir(parents=True, exist_ok=True)
    if any(capture.iterdir()):
        raise ValueError('collector capture must be empty')
    try:
        roots = {'venv': Path(_VENV).resolve(strict=True),
                 'base_stdlib': Path(sysconfig.get_path('stdlib')).resolve(strict=True)}
        if Path(sys.prefix).resolve(strict=True) != roots['venv']:
            raise ValueError('collector prefix differs from the declared venv')
        inputs = {source.name: _file_fact(source), 'mypy.ini': _file_fact(work / 'mypy.ini')}
        if source.read_bytes() != _STUB.encode() or (work / 'mypy.ini').read_bytes() != _CONFIG.encode():
            raise ValueError('collector fixed inputs changed')
        identity = {'schema': 'mamba.q0.164.pydantic-collector-identity.v1', 'family': _FAMILY,
            'python': list(sys.version_info[:3]), 'executable': sys.executable,
            'prefix': sys.prefix, 'base_prefix': sys.base_prefix, 'cwd': os.getcwd(),
            'flags': {name: getattr(sys.flags, name) for name in ('isolated', 'dont_write_bytecode', 'ignore_environment', 'no_user_site', 'safe_path')},
            'input_files': inputs, 'profile': {'path': str(profile), **_file_fact(profile)},
            'original_stdout_sha256': hashlib.sha256((json.dumps(original) + '\n').encode()).hexdigest(),
            'original_sidecars': {name: _file_fact(work / name) for name in sidecars}}
        _save_new(capture / 'collector-identity.json', identity)
        modules, unexpected, deferred = {}, [], []
        for name, module in tuple(sys.modules.items()):
            if module is None:
                modules[name] = {'path': None, 'category': 'none-module'}
                continue
            file = getattr(module, '__file__', None)
            if file:
                path = Path(file).resolve(strict=True)
                category = _path_category(path, roots, set())
                modules[name] = {'path': str(path), **_file_fact(path), 'category': category,
                    'native': any(str(path).endswith(suffix) for suffix in importlib.machinery.EXTENSION_SUFFIXES)}
                if category == 'unexpected':
                    unexpected.append(name)
                continue
            specification = getattr(module, '__spec__', None)
            origin = getattr(specification, 'origin', None)
            if origin in ('built-in', 'frozen'):
                modules[name] = {'path': None, 'category': origin}
            elif module is sys.modules.get('__main__'):
                modules[name] = {'path': None, 'category': 'entrypoint'}
            elif getattr(module, '__path__', None) is not None:
                paths = [Path(p).resolve(strict=True) for p in module.__path__]
                allowed = bool(paths) and all(_path_category(p, roots, set()) != 'unexpected' for p in paths)
                modules[name] = {'path': None, 'category': 'namespace' if allowed else 'unexpected',
                                 'namespace_paths': [str(p) for p in paths]}
                if not allowed:
                    unexpected.append(name)
            else:
                modules[name] = {'path': None, 'category': 'unknown-no-file', 'origin': origin}
                deferred.append((name, module))
        for name, module in deferred:
            alias = _object_alias(name, module, modules, sys.modules)
            if alias is None:
                unexpected.append(name)
            else:
                modules[name] = alias
        _save_new(capture / 'import-custody.json', {'schema': 'mamba.q0.164.import-custody.v1',
            'allowed_roots': {name: str(path) for name, path in roots.items()}, 'modules': modules,
            'file_backed_module_count': sum(row['path'] is not None for row in modules.values()), 'unexpected': unexpected})
        graph = {}
        graph_bad = []
        declared = {source.resolve(strict=True), (work / 'mypy.ini').resolve(strict=True)}
        for name, state in result.graph.items():
            if not state.path:
                graph[name] = {'path': None, 'category': 'no-source-path'}
                continue
            path = Path(state.path).resolve(strict=True)
            category = _path_category(path, roots, declared)
            graph[name] = {'path': str(path), **_file_fact(path), 'category': category}
            if category == 'unexpected':
                graph_bad.append(name)
        _save_new(capture / 'build-graph.json', {'schema': 'mamba.q0.164.build-graph.v1',
            'modules': graph, 'unexpected': graph_bad})
        if unexpected or graph_bad:
            raise ValueError('import or build graph escaped the declared reference roots')
    except Exception as failure:
        _save_new(capture / 'failure.json', {'type': type(failure).__name__, 'message': str(failure)})
        raise
'''

_FIRST_MAIN = r'''
assert sys.flags.isolated == 1
assert importlib.metadata.version('mypy') == '2.4.0'
assert importlib.metadata.version('pydantic') == '2.13.5'
minor, work, profile = int(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
assert sys.version_info[:2] == (3, minor)
assert define_options()[1] == json.loads(profile.read_text())['mypy']['strict_flags']
source = work / 'field_types.pyi'
config = work / 'mypy.ini'
assert source.read_bytes() == _STUB.encode()
assert config.read_bytes() == _CONFIG.encode()
sources, options = process_options(['--strict', '--python-version', f'3.{minor}', '--cache-dir=/dev/null',
    '--no-incremental', '--config-file', str(config), str(source)])
options.preserve_asts = True
result = build(sources=sources, options=options)
assert not result.errors, result.errors
field_type = result.graph['pydantic.fields'].tree.names['Field'].node.type.with_unpacked_kwargs()
variants = [pretty_callable(item, options) for item in field_type.items]
aliases, remember = _alias_reader(result.graph, _FAMILY == 'model-methods')
remember(field_type.serialize())
model = None
if _FAMILY == 'model-types':
    model = _basic_models(result.graph, remember)
elif _FAMILY == 'model-methods':
    base, model = _initial_methods(result.graph, remember)
    from mypy.nodes import Var, Decorator, FuncBase
    model['info_classes'] = _info_rows(result.graph, remember, Var, (Decorator, FuncBase))
    _remaining_methods(result.graph, base, model, remember)
assert len(variants) == 6
fields = Path(result.graph['pydantic.fields'].path)
record = {'python': list(sys.version_info[:3]), 'mypy': '2.4.0', 'pydantic': '2.13.5',
    'profile_sha256': hashlib.sha256(profile.read_bytes()).hexdigest(),
    'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
    'fields_path': str(fields), 'fields_sha256': hashlib.sha256(fields.read_bytes()).hexdigest(),
    'serialized_type': field_type.serialize()}
if model is not None:
    record['model_types'] = model
record.update(aliases=aliases, variants=variants, project_code_executed=False, development_only=True, accepted=False)
print(json.dumps(record))
_capture(result, work, profile, source, record, ())
'''

_CONFIG_MAIN = r'''
work, minor, profile = Path(sys.argv[1]), int(sys.argv[2]), Path(sys.argv[3])
assert sys.version_info[:2] == (3, minor)
assert importlib.metadata.version('mypy') == '2.4.0'
assert importlib.metadata.version('pydantic') == '2.13.5'
assert define_options()[1] == json.loads(profile.read_bytes())['mypy']['strict_flags']
work.mkdir(parents=True, exist_ok=False)
source, config = work / 'config_field.pyi', work / 'mypy.ini'
source.write_text(_STUB)
config.write_text(_CONFIG)
sources, options = process_options(['--strict', '--python-version', f'3.{minor}', '--cache-dir=/dev/null',
    '--no-incremental', '--config-file', str(config), str(source)])
options.preserve_asts = True
result = build(sources=sources, options=options)
assert not result.errors, result.errors
field = result.graph['pydantic.main'].tree.names['BaseModel'].node.names['model_config'].node
assert isinstance(field, Var)
value = {'type': field.type.serialize(), 'class_var': field.is_classvar, 'final': field.is_final}
(work / 'field.json').write_text(json.dumps(value, indent=2) + '\n')
package = Path(pydantic.__file__).resolve().parent
(work / 'identity.json').write_text(json.dumps({'mypy': '2.4.0', 'pydantic': '2.13.5',
    'profile_sha256': hashlib.sha256(profile.read_bytes()).hexdigest(),
    'source_sha256': {name: hashlib.sha256((package / name).read_bytes()).hexdigest()
                      for name in ('main.py', 'config.py', 'mypy.py')}}, indent=2) + '\n')
record = {'python': f'3.{minor}', 'class_var': field.is_classvar}
print(json.dumps(record))
_capture(result, work, profile, source, record, ('field.json', 'identity.json'))
'''


def render_collector(family: str, capture: Path, venv: Path) -> str:
    if family not in FAMILIES:
        raise ValueError("unknown fixed Pydantic asset family")
    if not isinstance(capture, Path) or not isinstance(venv, Path) or not capture.is_absolute() or not venv.is_absolute():
        raise ValueError("collector capture and venv paths must be absolute")
    settings = (f"_FAMILY = {family!r}\n_CAPTURE = {str(capture)!r}\n_VENV = {str(venv)!r}\n"
                f"_STUB = {STUBS[family]!r}\n_CONFIG = {CONFIG!r}\n"
                f"_METHOD_NAMES = {METHODS!r}\n_LEGACY_NAMES = {LEGACY_METHODS!r}\n")
    if family == 'model-config-field':
        imports = ("from pathlib import Path\nimport hashlib, importlib.metadata, json, sys\nimport pydantic\n"
                   "from mypy.build import build\nfrom mypy.main import define_options, process_options\nfrom mypy.nodes import Var\n")
        return imports + settings + _METADATA + _CONFIG_MAIN
    imports = ("import hashlib, importlib.metadata, json, sys\nfrom pathlib import Path\n"
               "from mypy.main import define_options, process_options\nfrom mypy.build import build\n"
               "from mypy.messages import pretty_callable\n")
    return imports + settings + _READERS + _METADATA + _FIRST_MAIN


def reference_argv(executable: Path, family: str, minor: int, work: Path,
                   profile: Path, capture: Path) -> list[str]:
    if type(minor) is not int or minor not in VERSIONS:
        raise ValueError("Pydantic target minor must be12,13or14")
    if any(not isinstance(path, Path) or not path.is_absolute() for path in (executable, work, profile, capture)):
        raise ValueError("Pydantic command paths must be absolute")
    code = render_collector(family, capture, executable.parent.parent)
    if family == 'model-config-field':
        return [str(executable), '-I', '-B', '-c', code, str(work), str(minor), str(profile)]
    return [str(executable), '-I', '-c', code, str(minor), str(work), str(profile)]
