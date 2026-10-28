"""Pure controls. No audited package is imported or executed."""
from __future__ import annotations
import base64
import copy
import csv
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import q0_pydantic_reference as runner
from q0_pydantic_inputs import FAMILIES, VERSIONS, input_spec, materialize_work


def make_identity(root):
    venv=root/'venv';site=venv/'lib/python3.12/site-packages';site.mkdir(parents=True)
    pins=[{'name':n,'version':v} for n,v in runner.PINS.items()]
    distributions,modules={},{}
    for pin in pins:
        name=pin['name'];canonical=name.replace('_','-');module=site/(name+'.py');module.write_bytes(b'harmless payload control')
        metadata=site/(name+'-'+pin['version']+'.dist-info');metadata.mkdir();record=str(metadata.relative_to(site)/'RECORD')
        fact=runner.file_fact(module)
        stream=io.StringIO(newline='');writer=csv.writer(stream,lineterminator='\r\n')
        writer.writerow([module.name,'sha256='+base64.urlsafe_b64encode(bytes.fromhex(fact['sha256'])).rstrip(b'=').decode(),str(fact['bytes'])])
        writer.writerow([record,'','']);raw=stream.getvalue().encode();(site/record).write_bytes(raw)
        record_fact={'bytes':len(raw),'sha256':runner.digest(raw)}
        distributions[canonical]={'name':name,'version':pin['version'],'requires_dist':[],'requires_python':'>=3.9',
             'metadata_path':str(metadata),'distribution_files':{module.name:fact,record:record_fact},
             'missing_distribution_files':[],'native_files':{},'installed_records':{record:{**record_fact,'bytes_b64':base64.b64encode(raw).decode()}}}
        modules[name]={'path':str(module),**fact}
    distributions['pydantic']['requires_dist']=['pydantic-core==2.46.5','typing-inspection>=0.4.2']
    distributions['typing-inspection']['requires_dist']=['typing-extensions>=4.15.0']
    markers={'python_full_version':'3.12.11','python_version':'3.12','sys_platform':'darwin','platform_system':'Darwin','platform_machine':'arm64'}
    base=root/'base';(base/'lib/python3.12').mkdir(parents=True)
    identity={'python':[3,12,11],'isolated':1,'prefix':str(venv),'base_prefix':str(base),'modules':modules,'distributions':distributions,
              'installed_site_distributions':{n.replace('_','-'):v for n,v in runner.PINS.items()},
              'mypy_version':'2.4.0','pydantic_version':'2.13.5','strict_flags':['control'],
              'dependency_verification':runner.validate_dependencies(pins,distributions,markers)}
    return identity,pins,venv


def fake_receipt(root,argv,stdout,stderr=b'',actual_exit=0):
    root.mkdir(parents=True)
    for name,value in [('stdout.raw',stdout),('stderr.raw',stderr),('inputs.json',b'{}'),
                       ('environment-before.json',b'{}'),('environment-after.json',b'{}')]:
        (root/name).write_bytes(value)
    stdin={'inheritance':'unmodified; no Popen stdin argument','contents':'not_captured'}
    runner.write_json(root/'stdin-identity.json',stdin)
    receipt={'argv':argv,'cwd':os.getcwd(),'stdin':stdin,'environment_inherited':True,'timeout_seconds':120,
        'pid':42,'waited_pid':42,'wait_completed':True,'timed_out':False,'launch_error':None,'actual_exit':actual_exit,
        'passed':actual_exit==0 and stderr==b''}
    for name,key in [('stdout.raw','stdout_sha256'),('stderr.raw','stderr_sha256'),('inputs.json','inputs_sha256'),
                     ('environment-before.json','environment_before_sha256'),('environment-after.json','environment_after_sha256'),
                     ('stdin-identity.json','stdin_identity_sha256')]:receipt[key]=runner.digest((root/name).read_bytes())
    runner.write_json(root/'receipt.json',receipt)
    return receipt


class Capture(unittest.TestCase):
    def test_main_inherits_cwd_env_stdin_without_inventing_input_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)/'command';argv=[sys.executable,'-c',"import os; print(os.environ['PYDANTIC_CAPTURE_CONTROL'])"]
            original=subprocess.Popen
            with patch.dict(os.environ,{'PYDANTIC_CAPTURE_CONTROL':'inherited'}),patch.object(runner.subprocess,'Popen',wraps=original) as launch:
                receipt=runner.run_inherited(argv,output,{'control':True});runner.require_main_receipt(receipt,output,argv)
            for name in ['stdin','cwd','env']:self.assertNotIn(name,launch.call_args.kwargs)
            self.assertFalse((output/'stdin.raw').exists());self.assertEqual((output/'stdout.raw').read_bytes(),b'inherited\n')
            self.assertEqual(receipt['stdin']['contents'],'not_captured');self.assertEqual(receipt['pid'],receipt['waited_pid'])

    def test_stderr_nonzero_exit_missing_wait_and_raw_hash_all_fail(self):
        for stderr,exitcode in [(b'',0),(b'warning',0),(b'',1)]:
            with self.subTest(stderr=stderr,exit=exitcode),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary)/'command';argv=['/mock/python','-I','-c','mock'];r=fake_receipt(root,argv,b'{}\n',stderr,exitcode)
                if stderr or exitcode:
                    with self.assertRaises(ValueError):runner.require_main_receipt(r,root,argv)
                else:
                    runner.require_main_receipt(r,root,argv)
                    for key,value in [('pid',None),('wait_completed',False),('waited_pid',43),('actual_exit',None),('timed_out',True)]:
                        with self.assertRaises(ValueError):runner.require_main_receipt({**r,key:value},root,argv)
                    (root/'stdout.raw').write_bytes(b'changed')
                    with self.assertRaises(ValueError):runner.require_main_receipt(r,root,argv)

    def test_timeout_retains_actual_wait_output_and_first_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)/'command';argv=[sys.executable,'-c',"import time; print('before',flush=True); time.sleep(30)"]
            r=runner.run_inherited(argv,root,{},timeout=.5)
            self.assertTrue(r['timed_out']);self.assertTrue(r['wait_completed']);self.assertEqual(r['pid'],r['waited_pid'])
            self.assertNotEqual(r['actual_exit'],0);self.assertEqual((root/'stdout.raw').read_bytes(),b'before\n')
            with self.assertRaises(ValueError):runner.require_main_receipt(r,root,argv)
            with self.assertRaises(FileExistsError):runner.run_inherited(argv,root,{})
            self.assertTrue(json.loads((root/'receipt.json').read_bytes())['timed_out'])

    def test_launch_failure_and_environment_privacy(self):
        with tempfile.TemporaryDirectory() as temporary:
            r=runner.run_inherited([str(Path(temporary)/'absent')],Path(temporary)/'command',{})
            self.assertIsNone(r['pid']);self.assertFalse(r['wait_completed']);self.assertIsNotNone(r['launch_error'])
        snapshot=runner.environment_snapshot({'PATH':'/bin','GITHUB_TOKEN':'private-token','SIGNED_URL':'secret-query'})
        self.assertEqual(snapshot['allowlisted_values'],{'PATH':'/bin'})
        self.assertEqual(snapshot['names'],['GITHUB_TOKEN','PATH','SIGNED_URL'])
        self.assertNotIn('private-token',json.dumps(snapshot));self.assertNotIn('secret-query',json.dumps(snapshot))


class PackageIdentity(unittest.TestCase):
    def test_all_ten_pins_complete_records_modules_and_constraints(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=make_identity(Path(temporary).resolve())
            runner.require_package_identity(identity,'3.12.11',pins,venv,['control'])
            self.assertEqual(len(runner.installed_payloads(identity,venv)),20)

    def test_five_default_pins_cannot_replace_the_frozen_ten(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=make_identity(Path(temporary).resolve())
            five=[p for p in pins if p['name'] in ['pydantic','pydantic_core','annotated-types','typing_extensions','typing-inspection']]
            with self.assertRaisesRegex(ValueError,'ten exact'):runner.require_package_identity(identity,'3.12.11',five,venv,['control'])

    def test_wrong_patch_pin_marker_flags_or_extra_distribution_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=make_identity(Path(temporary).resolve());changes=[]
            for key,value in [('python',[3,13,9]),('isolated',0),('strict_flags',[]),('mypy_version','2.3.1')]:
                altered=copy.deepcopy(identity);altered[key]=value;changes.append(altered)
            altered=copy.deepcopy(identity);altered['distributions']['pydantic-core']['version']='2.33.2';changes.append(altered)
            altered=copy.deepcopy(identity);altered['installed_site_distributions']['pip']='25.0.1';changes.append(altered)
            altered=copy.deepcopy(identity);altered['dependency_verification']['marker_environment']['sys_platform']='linux';changes.append(altered)
            for altered in changes:
                with self.assertRaises(ValueError):runner.require_package_identity(altered,'3.12.11',pins,venv,['control'])

    def test_declared_payload_hash_checked_even_when_record_itself_is_rehashed(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=make_identity(Path(temporary).resolve());d=identity['distributions']['pydantic'];record=next(iter(d['installed_records']))
            rows=list(csv.reader(io.StringIO(base64.b64decode(d['installed_records'][record]['bytes_b64']).decode())))
            rows[0][1]='sha256='+base64.urlsafe_b64encode(b'\0'*32).rstrip(b'=').decode()
            stream=io.StringIO(newline='');csv.writer(stream,lineterminator='\r\n').writerows(rows);raw=stream.getvalue().encode()
            (Path(d['metadata_path']).parent/record).write_bytes(raw);fact={'bytes':len(raw),'sha256':runner.digest(raw)}
            d['distribution_files'][record]=fact;d['installed_records'][record]={**fact,'bytes_b64':base64.b64encode(raw).decode()}
            with self.assertRaisesRegex(ValueError,'declared payload hash'):runner.require_package_identity(identity,'3.12.11',pins,venv,['control'])

    def test_missing_changed_payload_and_module_outside_record_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=make_identity(Path(temporary).resolve())
            altered=copy.deepcopy(identity);altered['distributions']['pydantic']['missing_distribution_files']=['missing']
            with self.assertRaises(ValueError):runner.require_package_identity(altered,'3.12.11',pins,venv,['control'])
            rogue=venv/'rogue.py';rogue.write_bytes(b'harmless rogue control')
            altered=copy.deepcopy(identity);altered['modules']['pydantic']={'path':str(rogue),**runner.file_fact(rogue)}
            with self.assertRaises(ValueError):runner.require_package_identity(altered,'3.12.11',pins,venv,['control'])
            Path(identity['modules']['pathspec']['path']).write_bytes(b'changed')
            with self.assertRaises(ValueError):runner.require_package_identity(identity,'3.12.11',pins,venv,['control'])

    def test_symlink_venv_must_resolve_to_the_exact_base(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);base=root/'base';base.write_bytes(b'control');python=root/'python';python.symlink_to(base)
            self.assertTrue(runner.require_venv_link(python,base)['venv_executable_is_symlink'])
            python.unlink();python.write_bytes(base.read_bytes())
            with self.assertRaises(ValueError):runner.require_venv_link(python,base)
            python.unlink();python.symlink_to(root/'missing')
            with self.assertRaises((ValueError,FileNotFoundError)):runner.require_venv_link(python,base)


class WorkAndProjection(unittest.TestCase):
    def test_exact_input_files_and_config_child_creation_boundary(self):
        profile=Path(__file__).resolve().parents[2]/'crates/mamba-quality/assets/profile.json'
        with tempfile.TemporaryDirectory() as temporary:
            for family in FAMILIES:
                work=Path(temporary).resolve()/family;materialize_work(family,work,profile)
                before=runner.inventory(work) if work.exists() else {}
                if family=='model-config-field':
                    self.assertFalse(work.exists());work.mkdir();s=input_spec(family)
                    (work/s['filename']).write_text(s['source']);(work/'mypy.ini').write_text(s['config'])
                    (work/'field.json').write_text('{}\n');(work/'identity.json').write_text('{}\n')
                runner.require_work_files(family,work,before,runner.inventory(work))
                (work/'arbitrary').write_bytes(b'new')
                with self.assertRaises(ValueError):runner.require_work_files(family,work,before,runner.inventory(work))
                (work/'arbitrary').unlink();(work/'mypy.ini').write_text('changed')
                with self.assertRaises(ValueError):runner.require_work_files(family,work,before,runner.inventory(work))

    def test_config_stdout_is_tiny_and_exact_original_transport(self):
        raw={'python':'3.12','class_var':False};transport=(json.dumps(raw)+'\n').encode()
        self.assertEqual(runner.decode_record('model-config-field',transport),raw)
        for bad in [b'',b'[]\n',json.dumps(raw).encode(),transport+b'\n',b'{"class_var":false,"python":"3.12"}\n']:
            with self.assertRaises((ValueError,json.JSONDecodeError)):runner.decode_record('model-config-field',bad)

    def test_original_package_source_and_all_six_overloads_are_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);package=root/'package';package.mkdir();fields=package/'fields.py';fields.write_bytes(b'fixed fields')
            profile=root/'profile.json';profile.write_bytes(b'profile');lock={'profile_sha256':runner.digest(b'profile'),'pydantic_fields_sha256':runner.digest(b'fixed fields')}
            raw={'python':[3,12,11],'mypy':'2.4.0','pydantic':'2.13.5','profile_sha256':lock['profile_sha256'],
                 'source_sha256':runner.declared_inputs('field-types')['field_types.pyi']['sha256'],'fields_path':str(fields),
                 'fields_sha256':lock['pydantic_fields_sha256'],'serialized_type':{},'aliases':{},'variants':['a','b','c','d','e','f'],
                 'project_code_executed':False,'development_only':True,'accepted':False}
            identity={'modules':{'pydantic':{'path':str(package/'__init__.py')}}}
            runner.validate_raw_identity('field-types',12,raw,root,profile,identity,lock)
            for key,value in [('variants',raw['variants'][:5]),('fields_sha256','0'*64),('python',[3,13,9]),('project_code_executed',True)]:
                with self.assertRaises(ValueError):runner.validate_raw_identity('field-types',12,{**raw,key:value},root,profile,identity,lock)

    def run_control_cell(self,root,mode):
        profile=Path(__file__).resolve().parents[2]/'crates/mamba-quality/assets/profile.json'
        environment=root/'environment';environment.mkdir();runner.write_json(environment/'identity.json',{})
        raw={'python':[3,12,11],'mypy':'2.4.0','pydantic':'2.13.5','profile_sha256':'control','source_sha256':'control',
             'fields_path':'/pinned/fields.py','fields_sha256':'control','serialized_type':{'id':1},'aliases':{},'variants':['a']*6,
             'project_code_executed':False,'development_only':True,'accepted':False}
        expected={k:raw[k] for k in ['serialized_type','aliases','variants','fields_sha256']}
        if mode=='mismatch':expected={**expected,'serialized_type':{'id':2}}
        if mode=='types':expected={**expected,'serialized_type':{'id':True}}
        def capture(argv,output,inputs,timeout):
            self.assertEqual(timeout,120)
            r=fake_receipt(output,argv,b'' if mode=='launch' else (json.dumps(raw)+'\n').encode(),actual_exit=1 if mode=='exit' else 0)
            if mode=='launch':r.update(pid=None,waited_pid=None,wait_completed=False,actual_exit=None,launch_error={'error_class':'OSError'},passed=False)
            (output/'receipt.json').write_text(json.dumps(r))
            return r
        with (patch.object(runner,'run_inherited',side_effect=capture),patch.object(runner,'validate_raw_identity',return_value=None),
              patch.object(runner,'validate_capture',return_value={})):
            return runner.run_cell('field-types',12,environment/'venv/bin/python',root/'cell',profile,expected,
                                   {'profile_sha256':runner.digest(profile.read_bytes())},environment)

    def test_complete_mismatch_and_python_equality_type_integrity_are_separate(self):
        for mode in ['match','mismatch','types','exit']:
            with self.subTest(mode=mode),tempfile.TemporaryDirectory() as temporary:
                row,projected=self.run_control_cell(Path(temporary).resolve(),mode)
                self.assertIsNotNone(projected);self.assertEqual(row['evidence_complete'],mode!='exit')
                self.assertEqual(row['raw_projected_equals_frozen'],mode!='mismatch')
                self.assertEqual(row['original_comparison_passed'],mode in ['match','types'])
                self.assertEqual(row['job_passed'],mode=='match')
                self.assertEqual(bool(row['secondary_type_errors']),mode=='types')

    def test_failed_launch_receipt_is_preserved_without_claiming_a_launched_process(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();row,projected=self.run_control_cell(root,'launch')
            self.assertIsNone(projected);self.assertEqual(row['observation_status'],'launch_failed')
            self.assertFalse(row['process_launched']);self.assertFalse(row['evidence_complete']);self.assertFalse(row['job_passed'])
            self.assertIsNotNone(row['command_pointer']);self.assertTrue((root/'cell/command/receipt.json').exists())
            self.assertEqual(row['errors'][0]['stage'],'original_process')


class Custody(unittest.TestCase):
    def setup_capture(self,root):
        identity,_,venv=make_identity(root);work=root/'work';capture=root/'capture';capture.mkdir()
        profile=Path(__file__).resolve().parents[2]/'crates/mamba-quality/assets/profile.json';materialize_work('field-types',work,profile);stdout=b'{}\n'
        observed={'family':'field-types','python':[3,12,11],'executable':str(venv/'bin/python'),'prefix':str(venv),
            'base_prefix':identity['base_prefix'],'cwd':os.getcwd(),'flags':{'isolated':1,'ignore_environment':1,'no_user_site':1,'safe_path':True,'dont_write_bytecode':0},
            'input_files':runner.declared_inputs('field-types'),'profile':{'path':str(profile),**runner.file_fact(profile)},
            'original_stdout_sha256':runner.digest(stdout),'original_sidecars':{}}
        modules={n:{**m,'category':'venv','native':False} for n,m in identity['modules'].items()};modules['sys']={'path':None,'category':'built-in'}
        custody={'allowed_roots':{'venv':str(venv),'base_stdlib':str(Path(identity['base_prefix'])/'lib/python3.12')},
                 'modules':modules,'unexpected':[],'file_backed_module_count':len(identity['modules'])}
        graph={'modules':{'field_types':{'path':str(work/'field_types.pyi'),**runner.file_fact(work/'field_types.pyi'),'category':'declared-input'},'pydantic':modules['pydantic']},'unexpected':[]}
        for name,obj in [('collector-identity.json',observed),('import-custody.json',custody),('build-graph.json',graph)]:runner.write_json(capture/name,obj)
        return identity,venv/'bin/python',work,capture,profile,stdout,observed,custody,graph

    def test_complete_import_graph_has_independent_file_joins(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,exe,work,capture,profile,stdout,*_=self.setup_capture(Path(temporary).resolve())
            result=runner.validate_capture('field-types',12,exe,work,capture,profile,identity,stdout)
            self.assertEqual(result['graph_node_count'],2);self.assertEqual(result['imported_file_count'],10)

    def add_alias(self,identity,custody):
        owner=Path(identity['base_prefix'])/'lib/python3.12/typing.py'
        owner.write_bytes(b'class io:\n    pass\nclass re:\n    pass\n')
        fact=runner.file_fact(owner)
        custody['modules']['typing']={'path':str(owner),**fact,'category':'base_stdlib','native':False}
        custody['file_backed_module_count']+=1
        alias={'path':None,'category':'object-alias','owner_module':'typing','owner_qualname':'io','owner_members':['io'],
               'identity_verified':True,'owner_path':str(owner),'owner_bytes':fact['bytes'],'owner_sha256':fact['sha256']}
        custody['modules']['typing.io']=alias
        custody['modules']['typing.re']={**alias,'owner_qualname':'re','owner_members':['re']}
        return alias

    def test_verified_fileless_owner_aliases_and_negative_cache_preserve_file_count(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,exe,work,capture,profile,stdout,_,custody,_=self.setup_capture(Path(temporary).resolve())
            self.add_alias(identity,custody)
            custody['modules']['negative.cache']={'path':None,'category':'none-module'}
            # Validation does not depend on the insertion order of owner and alias.
            custody['modules']=dict(reversed(list(custody['modules'].items())))
            (capture/'import-custody.json').write_text(json.dumps(custody))
            result=runner.validate_capture('field-types',12,exe,work,capture,profile,identity,stdout)
            self.assertEqual(result['imported_file_count'],11)

    def test_fileless_alias_requires_exact_verified_owner_member_path_and_hash(self):
        for change in ['unknown','unverified','missing-owner','wrong-owner','wrong-qualname','wrong-member','local-member',
                       'wrong-path','wrong-hash','wrong-size','changed-owner','base-global','extra-negative-field']:
            with self.subTest(change=change),tempfile.TemporaryDirectory() as temporary:
                identity,exe,work,capture,profile,stdout,_,custody,_=self.setup_capture(Path(temporary).resolve())
                alias=self.add_alias(identity,custody)
                if change=='unknown':alias['category']='unknown-no-file'
                elif change=='unverified':alias['identity_verified']=False
                elif change=='missing-owner':del custody['modules']['typing']
                elif change=='wrong-owner':alias['owner_module']='pydantic'
                elif change=='wrong-qualname':alias['owner_qualname']='other'
                elif change=='wrong-member':alias['owner_members']=['re']
                elif change=='local-member':alias['owner_members']=['<locals>']
                elif change=='wrong-path':alias['owner_path']=str(profile)
                elif change=='wrong-hash':alias['owner_sha256']='0'*64
                elif change=='wrong-size':alias['owner_bytes']+=1
                elif change=='changed-owner':Path(alias['owner_path']).write_bytes(b'changed')
                elif change=='base-global':
                    path=Path(identity['base_prefix'])/'lib/python3.12/nested/site-packages/typing.py';path.parent.mkdir(parents=True);path.write_bytes(b'outside')
                    custody['modules']['typing']={'path':str(path),**runner.file_fact(path),'category':'base_stdlib','native':False}
                    alias.update(owner_path=str(path),owner_bytes=path.stat().st_size,owner_sha256=runner.digest(path.read_bytes()))
                else:custody['modules']['negative.cache']={'path':None,'category':'none-module','sha256':'0'*64}
                (capture/'import-custody.json').write_text(json.dumps(custody))
                with self.assertRaises(ValueError):runner.validate_capture('field-types',12,exe,work,capture,profile,identity,stdout)

    def test_empty_escaped_changed_and_unhashed_custody_never_passes(self):
        for change in ['module-empty','module-escape','graph-empty','graph-escape','input-changed','stdout-hash','namespace-escape','base-global']:
            with self.subTest(change=change),tempfile.TemporaryDirectory() as temporary:
                identity,exe,work,capture,profile,stdout,observed,custody,graph=self.setup_capture(Path(temporary).resolve())
                if change=='module-empty':custody['modules']={};custody['file_backed_module_count']=0
                elif change=='module-escape':custody['modules']['bad']={'path':str(profile),**runner.file_fact(profile),'category':'venv'};custody['file_backed_module_count']+=1
                elif change=='graph-empty':graph['modules']={}
                elif change=='graph-escape':graph['modules']['bad']={'path':str(profile),**runner.file_fact(profile),'category':'venv'}
                elif change=='input-changed':(work/'field_types.pyi').write_bytes(b'changed')
                elif change=='stdout-hash':observed['original_stdout_sha256']='0'*64
                elif change=='base-global':
                    extra=Path(identity['base_prefix'])/'lib/python3.12/nested/dist-packages/rogue.py';extra.parent.mkdir(parents=True);extra.write_bytes(b'outside pinned packages')
                    custody['modules']['bad']={'path':str(extra),**runner.file_fact(extra),'category':'base_stdlib'};custody['file_backed_module_count']+=1
                else:custody['modules']['bad']={'path':None,'category':'namespace','namespace_paths':[str(profile.parent)]}
                for name,obj in [('collector-identity.json',observed),('import-custody.json',custody),('build-graph.json',graph)]:
                    (capture/name).write_text(json.dumps(obj))
                with self.assertRaises(ValueError):runner.validate_capture('field-types',12,exe,work,capture,profile,identity,stdout)


class Orchestration(unittest.TestCase):
    def test_twelve_cells_keep_family_target_order_after_one_failure(self):
        repository=Path(__file__).resolve().parents[2];lockpath=repository/'tools/ci/q0_pydantic_reference_lock.json';read=Path.read_bytes
        lock=json.loads(read(lockpath));lock['sdist']['bytes']=4
        assets={f:{'schema':'mamba.quality.pydantic-'+f+'.v1','mypy':'2.4.0','pydantic':'2.13.5',
                   'targets':{f'3.{m}':{'value':m} for m in VERSIONS}} for f in FAMILIES};called=[]
        def cell(family,minor,exe,path,profile,expected,lock,environment_root):
            called.append((family,minor))
            if len(called)==1:raise ValueError('first failure retained')
            row=runner.index_row(family,minor);row.update(observation_status='observed',process_launched=True,evidence_complete=True,
                original_comparison_passed=True,raw_projected_equals_frozen=True,job_passed=True,errors=[])
            return row,expected
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary).resolve()/'run';argv=['runner','--repository',str(repository),'--output',str(output),
                '--python312','/base312','--python313','/base313','--python314','/base314']
            with (patch.object(sys,'argv',argv),patch.object(runner,'require_host_context'),
                  patch.object(Path,'read_bytes',lambda p:json.dumps(lock).encode() if p==lockpath else read(p)),
                  patch('q0_pydantic_inputs.read_assets',return_value=assets),patch.object(runner,'download',return_value=b'fake'),
                  patch.object(runner,'prepare',side_effect=[Path('/venv312/bin/python'),Path('/venv313/bin/python'),Path('/venv314/bin/python')]),
                  patch.object(runner,'run_cell',side_effect=cell),patch('sys.stdout',new_callable=io.StringIO)):
                self.assertEqual(runner.main(),1)
            self.assertEqual(called,[(f,m) for f in FAMILIES for m in VERSIONS])
            rows=[json.loads(s) for s in (output/'index.jsonl').read_text().splitlines()];self.assertEqual(len(rows),12)
            self.assertFalse(rows[0]['job_passed']);self.assertEqual(rows[0]['observation_status'],'not_run')
            summary=json.loads((output/'summary.json').read_bytes());self.assertFalse(summary['job_passed'])
            self.assertEqual(summary['observed_original_processes'],11);self.assertTrue(summary['unique_ordered_coverage'])
            self.assertFalse(summary['full_reference_accepted']);self.assertEqual(summary['native_parity'],'not_run')


if __name__=='__main__':unittest.main()
