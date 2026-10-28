"""Pure renderer/fake-API controls. No reference library is imported or run."""
import ast
import copy
import hashlib
import importlib
import json
from pathlib import Path
from types import ModuleType, SimpleNamespace as Box
import tempfile
import typing
import unittest
import warnings
from unittest.mock import patch

from q0_pydantic_collectors import (CONFIG, FAMILIES, LEGACY_METHODS, METHODS, STUBS,
                                   reference_argv, render_collector)


def helpers(**extra):
    code = render_collector('model-methods', Path('/capture'), Path('/venv'))
    nodes = [n for n in ast.parse(code).body if isinstance(n, ast.FunctionDef)]
    scope = {'Path': Path, 'hashlib': hashlib, 'json': json, 'importlib': importlib,
             '_METHOD_NAMES': METHODS, '_LEGACY_NAMES': LEGACY_METHODS, **extra}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), '<pure collector helpers>', 'exec'), scope)
    return scope


class Serialized:
    def __init__(self, raw, unpacked=None):
        self.raw = raw
        self.unpacked = self if unpacked is None else unpacked
        self.calls = 0
        self.unpack_calls = 0

    def serialize(self):
        self.calls += 1
        return copy.deepcopy(self.raw)

    def with_unpacked_kwargs(self):
        self.unpack_calls += 1
        return self.unpacked


class Variable:
    def __init__(self, raw):
        self.type = None if raw is None else Serialized(raw)
        self.is_classvar = True
        self.is_final = False
        self.is_initialized_in_class = True


class Method:
    def __init__(self, raw):
        self.type = Serialized({'packed': raw}, Serialized({'unpacked': raw}))
        self.is_property, self.is_class, self.is_static = True, False, False
        self.line = 17


def symbols(**nodes):
    return {name: Box(node=node, plugin_generated=False, module_public=True) for name, node in nodes.items()}


def state(names, path=None):
    return Box(tree=Box(names=names), path=path)


class RendererControls(unittest.TestCase):
    def test_exact_four_family_argv_and_no_added_isolation_flags(self):
        exe, work, profile, capture = (Path(p) for p in ('/venv/bin/python', '/cell/work', '/profile.json', '/cell/capture'))
        for family in FAMILIES:
            argv = reference_argv(exe, family, 13, work, profile, capture)
            if family == 'model-config-field':
                self.assertEqual(argv[:4], [str(exe), '-I', '-B', '-c'])
                self.assertEqual(argv[5:], [str(work), '13', str(profile)])
                source = argv[4]
            else:
                self.assertEqual(argv[:3], [str(exe), '-I', '-c'])
                self.assertEqual(argv[4:], ['13', str(work), str(profile)])
                source = argv[3]
            ast.parse(source)
            self.assertIn("_VENV = '/venv'", source)

    def test_unknown_zero_or_different_targets_and_relative_paths_fail(self):
        paths = [Path('/venv/bin/python'), 'field-types', 12, Path('/work'), Path('/profile'), Path('/capture')]
        for index, value in ((1, 'other'), (2, 0), (2, True), (2, 11), (0, Path('python')),
                             (3, Path('work')), (4, Path('profile')), (5, Path('capture'))):
            with self.subTest(index=index, value=value), self.assertRaises(ValueError):
                changed = list(paths)
                changed[index] = value
                reference_argv(*changed)

    def test_original_stub_and_config_byte_boundaries(self):
        self.assertEqual([len(STUBS[f].encode()) for f in FAMILIES], [36, 86, 86, 70])
        self.assertEqual(len(CONFIG.encode()), 127)
        self.assertTrue(all(s.endswith('\n') for s in STUBS.values()))
        self.assertEqual(CONFIG, '[mypy]\nplugins = pydantic.mypy\n[pydantic-mypy]\ninit_typed = true\ninit_forbid_extra = true\nwarn_required_dynamic_aliases = true\n')

    def test_first_three_do_not_create_or_rewrite_work_inputs(self):
        for family in FAMILIES[:3]:
            code = render_collector(family, Path('/capture'), Path('/venv'))
            tree = ast.parse(code)
            top_calls = [n for node in tree.body if not isinstance(node, ast.FunctionDef) for n in ast.walk(node)
                         if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute)]
            self.assertFalse(any(n.func.attr in ('mkdir', 'write_text', 'write_bytes') for n in top_calls))
            self.assertIn('assert source.read_bytes() == _STUB.encode()', code)

    def test_fourth_creates_fresh_root_and_keeps_authoritative_sidecars(self):
        code = render_collector(FAMILIES[3], Path('/capture'), Path('/venv'))
        self.assertIn('work.mkdir(parents=True, exist_ok=False)', code)
        self.assertIn("work / 'config_field.pyi'", code)
        self.assertIn("(work / 'field.json').write_text", code)
        self.assertIn("(work / 'identity.json').write_text", code)
        self.assertIn("record = {'python': f'3.{minor}', 'class_var': field.is_classvar}", code)

    def test_original_result_print_precedes_metadata_only_custody(self):
        for family in FAMILIES:
            tree = ast.parse(render_collector(family, Path('/capture'), Path('/venv')))
            calls = [n.value for n in tree.body if isinstance(n, ast.Expr) and isinstance(n.value, ast.Call)]
            print_index = next(i for i, n in enumerate(calls) if isinstance(n.func, ast.Name) and n.func.id == 'print')
            capture_index = next(i for i, n in enumerate(calls) if isinstance(n.func, ast.Name) and n.func.id == '_capture')
            self.assertLess(print_index, capture_index)
            top_imports = [n for n in tree.body if isinstance(n, (ast.Import, ast.ImportFrom))]
            self.assertFalse(any(isinstance(n, ast.Import) and any(x.name in ('os', 'sysconfig') for x in n.names) for n in top_imports))


class PublicObjectControls(unittest.TestCase):
    def test_alias_graph_keeps_ids_order_cycles_and_every_nested_reference(self):
        alias = lambda name: {'.class': 'TypeAliasType', 'type_ref': name}
        graph = {'a': state(symbols(A=Box(target=Serialized({'children': [alias('b.B'), alias('a.A')], 'id': -17})))),
                 'b': state(symbols(B=Box(target=Serialized({'literal': 'unchanged', 'id': 44}))))}
        found, add = helpers()['_alias_reader'](graph, False)
        add([alias('a.A'), {'again': alias('b.B')}])
        self.assertEqual(list(found), ['a.A', 'b.B'])
        self.assertEqual(found['a.A']['id'], -17)
        self.assertEqual(found['b.B']['id'], 44)

    def test_typed_dict_alias_fallback_only_for_methods_recipe(self):
        graph = {'a': state(symbols(A=Box(typeddict_type=Serialized({'items': ['x']}))))}
        reference = {'.class': 'TypeAliasType', 'type_ref': 'a.A'}
        found, add = helpers()['_alias_reader'](graph, True)
        add(reference)
        self.assertEqual(found, {'a.A': {'items': ['x']}})
        with self.assertRaises(AttributeError):
            helpers()['_alias_reader'](graph, False)[1](reference)

    def test_basic_models_keep_constructor_config_and_bases_unchanged(self):
        init = Serialized({'id': -2})
        typed = Serialized({'required': ['z', 'a']})
        graph = {'pydantic.main': state(symbols(BaseModel=Box(fullname='pydantic.main.BaseModel', names=symbols(__init__=Box(type=init)), bases=[Serialized('base')]))),
                 'pydantic.config': state(symbols(ConfigDict=Box(fullname='pydantic.config.ConfigDict', typeddict_type=typed)))}
        seen = []
        value = helpers()['_basic_models'](graph, seen.append)
        self.assertEqual(list(value), ['base_model', 'config_dict'])
        self.assertEqual(value['base_model']['init_type'], {'id': -2})
        self.assertEqual(value['config_dict']['type']['required'], ['z', 'a'])
        self.assertEqual(seen, [{'id': -2}, {'required': ['z', 'a']}])

    def info_graph(self, unknown=False):
        infos = {}
        for name in ('FieldInfo', 'ComputedFieldInfo'):
            init = Method(name + '.init')
            entries = symbols(__init__=init, _private=Variable(99), untyped=Variable(None),
                              z_field=Variable({'id': -99}), a_method=Method(name + '.call'))
            if unknown:
                entries['unsupported'] = Box(node=object())
            infos[name] = Box(fullname='pydantic.fields.' + name, names=entries,
                              bases=[Serialized('base')], slots={'z', 'a'} if name == 'FieldInfo' else None,
                              is_final=True)
        fields = state(symbols(**infos))
        return {'pydantic.fields': fields, 'pydantic': state({'FieldInfo': Box(module_public=False)})}

    def test_info_rows_keep_public_order_unpack_only_original_calls_and_sort_slots(self):
        graph = self.info_graph()
        rows = helpers()['_info_rows'](graph, lambda value: None, Variable, (Method,))
        field = rows['FieldInfo']
        self.assertEqual(list(field['members']), ['z_field', 'a_method'])
        self.assertEqual(field['members']['z_field']['type'], {'id': -99})
        self.assertEqual(field['members']['a_method']['type'], {'unpacked': 'FieldInfo.call'})
        self.assertEqual(field['slots'], ['a', 'z'])
        self.assertIsNone(rows['ComputedFieldInfo']['slots'])
        self.assertEqual(field['raw_initializer'], {'packed': 'FieldInfo.init'})
        self.assertEqual(field['initializer'], {'unpacked': 'FieldInfo.init'})
        self.assertEqual(field['initializer_line'], 17)
        self.assertIs(field['initializer_generated'], False)
        self.assertEqual(field['exports'], {'pydantic': False, 'pydantic.fields': True})
        self.assertIs(rows['ComputedFieldInfo']['exports']['pydantic'], False)

    def test_unexpected_public_info_node_is_never_silently_filtered(self):
        with self.assertRaises(AssertionError):
            helpers()['_info_rows'](self.info_graph(True), lambda value: None, Variable, (Method,))

    def test_method_flags_require_original_attributes_and_keep_serialized_values(self):
        method = Method('M')
        result = helpers()['_method_row'](method)
        self.assertEqual(result, {'type': {'packed': 'M'}, 'property': True, 'class_method': False, 'static_method': False})
        self.assertEqual(method.type.unpack_calls, 0)
        with self.assertRaises(AttributeError):
            helpers()['_method_row'](Box(type=Serialized('no flags')))
        self.assertEqual(helpers()['_method_row'](Box(type=Serialized('optional')), True)['property'], False)

    def test_all_base_method_selectors_and_original_serialization_order(self):
        methods = {name: Method(name) for name in METHODS}
        names = symbols(**methods)
        names.update(symbols(model_fields=Variable('fields'), model_computed_fields=Variable('computed')))
        base = Box(fullname='pydantic.main.BaseModel', names=names)
        schema = Box(fullname='pydantic.json_schema.GenerateJsonSchema',
                     names=symbols(__init__=Method('schema')), bases=[Serialized('object')])
        descriptor = Box(fullname='descriptor', defn=Box(type_vars=[Serialized('T')]),
                         bases=[Serialized('object')], names=symbols(__init__=Method('init'), __get__=Method('get')))
        graph = {'pydantic.main': state(symbols(BaseModel=base)),
                 'pydantic.json_schema': state(symbols(GenerateJsonSchema=schema)),
                 'pydantic._internal._utils': state(symbols(deprecated_instance_property=descriptor))}
        _, result = helpers()['_initial_methods'](graph, lambda value: None)
        self.assertEqual(list(result['methods']), list(METHODS))
        self.assertEqual(len(result['methods']), 26)
        self.assertTrue(all(method.type.calls == 2 and method.type.unpack_calls == 0 for method in methods.values()))
        self.assertEqual(list(result['class_properties']), ['model_fields', 'model_computed_fields'])
        self.assertEqual(result['schema_generator']['init_type'], {'packed': 'schema'})

    def test_remaining_selectors_preserve_protocol_scalar_root_and_legacy_metadata(self):
        base = Box(names=symbols(**{name: Method(name) for name in METHODS}))
        scalar = {name: Box(names=symbols(**{member: Method(name + member) for member in ('__gt__', '__ge__', '__lt__', '__le__')}))
                  for name in ('int', 'float', 'str', 'bytes')}
        protocols = {name: Box(fullname=name, is_protocol=True, names=symbols(**{member: Method(name)}))
                     for name, member in (('SupportsGt', '__gt__'), ('SupportsGe', '__ge__'), ('SupportsLt', '__lt__'), ('SupportsLe', '__le__'))}
        cached = Box(fullname='functools.cached_property', defn=Box(type_vars=[Serialized('T')]), bases=[],
                     names=symbols(**{name: Method(name) for name in ('__init__', '__get__', '__set__')}))
        root = Box(fullname='pydantic.root_model.RootModel', defn=Box(type_vars=[Serialized('T')]), bases=[],
                   metadata={'pydantic-mypy-metadata': {'fields': ['z', 'a']}},
                   names={**symbols(**{name: Method(name) for name in ('__init__', 'model_construct', 'model_dump', '__copy__', '__deepcopy__')}),
                          **symbols(root=Box(type=Serialized({'id': -7})))})
        graph = {'pydantic': state(symbols(__getattr__=Box(type=Serialized('getattr')))),
                 'annotated_types': state(symbols(**protocols)), 'builtins': state(symbols(**scalar)),
                 'pydantic.fields': state(symbols(computed_field=Box(type=Serialized({'id': -8})))),
                 'functools': state(symbols(cached_property=cached)), 'pydantic.root_model': state(symbols(RootModel=root))}
        output = {'methods': {name: {'original': name} for name in METHODS}}
        helpers()['_remaining_methods'](graph, base, output, lambda value: None)
        self.assertEqual(list(output['comparison_protocols']), ['SupportsGt', 'SupportsGe', 'SupportsLt', 'SupportsLe'])
        self.assertEqual(list(output['scalar_comparisons']), ['int', 'float', 'str', 'bytes'])
        self.assertEqual(output['computed_field'], {'id': -8})
        self.assertEqual(output['root_model']['root_type'], {'id': -7})
        self.assertEqual(output['root_model']['metadata'], {'fields': ['z', 'a']})
        self.assertEqual(list(output['root_model']['methods']), ['__init__', 'model_construct', 'model_dump', '__copy__', '__deepcopy__'])
        self.assertEqual([name for name, row in output['methods'].items() if 'node_kind' in row], list(LEGACY_METHODS))


class CustodyControls(unittest.TestCase):
    def test_real_stdlib_type_aliases_have_identical_pinned_owner_and_no_warnings(self):
        owner = Path(typing.__file__).resolve()
        raw = owner.read_bytes()
        facts = {'typing': {'path': str(owner), 'bytes': len(raw),
                            'sha256': hashlib.sha256(raw).hexdigest(), 'category': 'base_stdlib'}}
        names = ['Any'] + [name for name in ('io', 're') if name in vars(typing)]
        with warnings.catch_warnings():
            warnings.simplefilter('error', DeprecationWarning)
            for name in names:
                value = vars(typing)[name]
                row = helpers()['_object_alias']('typing.' + name, value, facts, {'typing': typing})
                self.assertIsNotNone(row)
                self.assertEqual(row['owner_members'], [name])
                self.assertEqual(row['owner_path'], str(owner))
                self.assertEqual(row['owner_sha256'], facts['typing']['sha256'])
                self.assertIs(row['identity_verified'], True)

    def test_alias_owner_identity_mismatch_unknown_owner_and_wrong_name_fail(self):
        owner = ModuleType('owner')
        genuine = type('Member', (), {'__module__': 'owner', '__qualname__': 'Member'})
        forged = type('Member', (), {'__module__': 'owner', '__qualname__': 'Member'})
        owner.Member = genuine
        facts = {'owner': {'path': '/verified/owner.py', 'bytes': 10, 'sha256': 'a' * 64, 'category': 'venv'}}
        verify = helpers()['_object_alias']
        self.assertIsNone(verify('owner.Member', forged, facts, {'owner': owner}))
        self.assertIsNone(verify('other.Member', genuine, facts, {'owner': owner}))
        self.assertIsNone(verify('owner.Member', genuine, {}, {'owner': owner}))
        self.assertIsNone(verify('owner.Member', object(), facts, {'owner': owner}))
        self.assertIsNone(verify('owner.Member', genuine, {'owner': {**facts['owner'], 'category': 'unexpected'}}, {'owner': owner}))

    def test_nested_alias_follows_actual_namespace_dictionaries(self):
        owner = ModuleType('owner')
        inner = type('Inner', (), {'__module__': 'owner', '__qualname__': 'Outer.Inner'})
        owner.Outer = Box(Inner=inner)
        facts = {'owner': {'path': '/verified/owner.py', 'bytes': 10, 'sha256': 'a' * 64, 'category': 'venv'}}
        row = helpers()['_object_alias']('owner.Outer.Inner', inner, facts, {'owner': owner})
        self.assertEqual(row['owner_members'], ['Outer', 'Inner'])

    def test_unknown_path_and_symlink_escape_are_rejected_without_executing_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            venv = root / 'venv'
            venv.mkdir()
            outside = root / 'outside.py'
            outside.write_bytes(b'synthetic bytes; not executed')
            link = venv / 'alias.py'
            link.symlink_to(outside)
            categorize = helpers()['_path_category']
            self.assertEqual(categorize(outside, {'venv': venv}, set()), 'unexpected')
            self.assertEqual(categorize(link, {'venv': venv}, set()), 'unexpected')
            self.assertEqual(categorize(outside, {'venv': venv}, {outside}), 'declared-input')
            base_site = root / 'base/site-packages'
            base_site.mkdir(parents=True)
            unpinned = base_site / 'unpinned.py'
            unpinned.write_bytes(b'not a standard-library payload')
            self.assertEqual(categorize(unpinned, {'base_stdlib': root / 'base'}, set()), 'unexpected')

    def test_capture_records_input_profile_module_graph_and_original_stdout_hash(self):
        self.run_capture(False)

    def test_capture_retains_unexpected_module_and_failure_instead_of_passing(self):
        self.run_capture(True)

    def test_none_module_cache_has_no_loaded_source_and_is_not_an_escape(self):
        self.run_capture(False, none_cache=True)

    def test_alias_before_owner_in_module_order_is_verified_after_file_custody(self):
        self.run_capture(False, alias_cache=True)

    def test_unknown_fileless_object_still_fails_full_capture(self):
        self.run_capture(False, unknown_cache=True)

    def run_capture(self, outside, none_cache=False, alias_cache=False, unknown_cache=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            work, venv, base, capture = (root / p for p in ('work', 'venv', 'base', 'capture'))
            for path in (work, venv, base, capture):
                path.mkdir()
            source = work / 'field_types.pyi'
            source.write_text(STUBS['field-types'])
            (work / 'mypy.ini').write_text(CONFIG)
            profile = root / 'profile.json'
            profile.write_bytes(b'{}')
            library = (root if outside else venv) / 'library.py'
            library.write_bytes(b'synthetic library bytes; never imported')
            graph = Box(graph={'input': Box(path=str(source)), 'synthetic': Box(path=None)})
            fake_sys = Box(prefix=str(venv), base_prefix=str(base), executable=str(venv / 'bin/python'),
                           version_info=(3, 12, 11), modules={'observed': Box(__file__=str(library))},
                           flags=Box(isolated=1, dont_write_bytecode=0, ignore_environment=1, no_user_site=1, safe_path=True))
            if none_cache:
                fake_sys.modules['negative_cache'] = None
            if alias_cache:
                owner = ModuleType('owner')
                owner.__file__ = str(library)
                owner.Member = type('Member', (), {'__module__': 'owner', '__qualname__': 'Member'})
                fake_sys.modules = {'owner.Member': owner.Member, 'owner': owner, **fake_sys.modules}
            if unknown_cache:
                fake_sys.modules['unknown_cache'] = object()
            scope = helpers(sys=fake_sys, _CAPTURE=str(capture), _VENV=str(venv), _FAMILY='field-types',
                            _STUB=STUBS['field-types'], _CONFIG=CONFIG)
            original = {'value': 'kept'}
            with patch('sysconfig.get_path', return_value=str(base)):
                if outside or unknown_cache:
                    with self.assertRaises(ValueError):
                        scope['_capture'](graph, work, profile, source, original, ())
                else:
                    scope['_capture'](graph, work, profile, source, original, ())
            identity = json.loads((capture / 'collector-identity.json').read_bytes())
            self.assertEqual(identity['original_stdout_sha256'], hashlib.sha256((json.dumps(original) + '\n').encode()).hexdigest())
            self.assertEqual(identity['python'], [3, 12, 11])
            self.assertEqual(identity['input_files']['field_types.pyi']['bytes'], 36)
            imports = json.loads((capture / 'import-custody.json').read_bytes())
            self.assertEqual(imports['unexpected'], (['observed'] if outside else []) + (['unknown_cache'] if unknown_cache else []))
            self.assertEqual(imports['modules']['observed']['path'], str(library))
            if none_cache:
                self.assertEqual(imports['modules']['negative_cache'], {'path': None, 'category': 'none-module'})
            if alias_cache:
                alias = imports['modules']['owner.Member']
                self.assertEqual(alias, {'path': None, 'category': 'object-alias', 'owner_module': 'owner',
                    'owner_qualname': 'Member', 'owner_members': ['Member'], 'identity_verified': True,
                    'owner_path': str(library), 'owner_bytes': len(library.read_bytes()),
                    'owner_sha256': hashlib.sha256(library.read_bytes()).hexdigest()})
            graph_value = json.loads((capture / 'build-graph.json').read_bytes())
            self.assertEqual(graph_value['modules']['input']['category'], 'declared-input')
            self.assertEqual(graph_value['modules']['synthetic'], {'path': None, 'category': 'no-source-path'})
            self.assertEqual((capture / 'failure.json').exists(), outside or unknown_cache)
            self.assertEqual(source.read_text(), STUBS['field-types'])


if __name__ == '__main__':
    unittest.main()
