"""Static bindings and mocked adapter controls; no reference package executes."""
from __future__ import annotations

import ast
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import q0_16_contract_isort as adapter

REPO=Path(__file__).resolve().parents[2]


def api_actual(request):
    expected=request["expected"][0]["value"];output=expected.get("output")
    return {"request_id":request["request_id"],"target":request["target"],"operation":"isort_code_API",
        "input_sha256":adapter.digest(bytes.fromhex(request["source_hex"])),"api_calls":1,
        "output":output,"output_hex":output.encode().hex() if output is not None else None,
        "error_class":expected.get("error_class"),"error_module":"isort.exceptions" if output is None else None,
        "message":expected.get("message"),"version":"9.0.2","api_stdout_hex":"","api_stderr_hex":""}


def capture_fake(request, *, saved="valid", change=None, private_cr=False):
    def run(argv,cwd,destination,**options):
        destination.mkdir()
        source=cwd/request["filename"]
        stdout,stderr=b"",b"";actual_exit=0
        if request["operation"]=="isort_code_API":
            actual=api_actual(request)
            if change:change(actual,cwd)
            stdout=adapter.encoded(actual)+b"\n"
        else:
            expected=adapter.cli_expected(request);index=adapter.STAGES.index(destination.name)
            actual_exit=expected["exits"][index];before=source.read_bytes()
            if destination.name=="format":
                output=bytes.fromhex(request["expected"][0]["value"]["private_code_api_output_hex"] if private_cr else expected["output_hex"])
                source.write_bytes(output)
                if output!=before:stdout=f"Fixing {source}\n".encode()
            elif actual_exit==1:stderr=f"ERROR: {source} Imports are incorrectly sorted and/or formatted.\n".encode()
        adapter.write_json(destination/"inputs.json",options["inputs"])
        (destination/"stdin.raw").write_bytes(b"");(destination/"stdout.raw").write_bytes(stdout);(destination/"stderr.raw").write_bytes(stderr)
        returned={"argv":argv,"cwd":str(cwd),"expected_exits":options["expected_exits"],"started_utc":"2026-10-29T00:00:00+00:00",
            "pid":123,"waited_pid":123,"wait_completed":True,"actual_exit":actual_exit,"timed_out":False,"launch_error":None,
            "duration_seconds":0.1,"stdout_sha256":adapter.digest(stdout),"stderr_sha256":adapter.digest(stderr),
            "stdin_sha256":adapter.digest(b""),"inputs_sha256":adapter.digest((destination/"inputs.json").read_bytes()),"passed":True}
        stored=copy.deepcopy(returned)
        if saved=="bool_tamper":stored["passed"]=1
        if saved in {"valid","bool_tamper"}:adapter.write_json(destination/"receipt.json",stored)
        elif saved=="corrupt":(destination/"receipt.json").write_bytes(b"{invalid")
        return returned
    return run


def prepared_fixture(root,version,*,wrong_installer=False,extra_distribution=False,wrong_probe_origin=False):
    """Create only inert files; mocked commands do not import or execute them."""
    minor=".".join(version.split(".")[:2]);base=root/"pbs/python";python=base/"bin"/("python"+minor)
    python.parent.mkdir(parents=True);python.write_bytes(b"inert pinned interpreter")
    installer=base/"lib"/("python"+minor)/"ensurepip/_bundled/pip-25.0-py3-none-any.whl"
    installer.parent.mkdir(parents=True);installer.write_bytes(b"inert sealed installer wheel")
    archive_sha=adapter.digest(b"inert sealed archive")
    interpreter_pin={"executable_sha256":adapter.digest(python.read_bytes()),"sha256":archive_sha}
    installer_pin={"archive_sha256":archive_sha,"member":installer.relative_to(root/"pbs").as_posix(),
        "bytes":installer.stat().st_size,"sha256":adapter.digest(b"wrong installer" if wrong_installer else installer.read_bytes())}
    wheels=[];pins=[]
    for name,version_pin in (("isort","9.0.2"),("mypy_extensions","1.1.0")):
        path=root/(name+".whl");path.write_bytes(("inert wheel "+name).encode());wheels.append(path)
        pins.append({"name":name,"version":version_pin,"filename":path.name,"sha256":adapter.digest(path.read_bytes())})
    environment=root/"environment"
    def command(argv,cwd,destination,**options):
        destination.mkdir();stdout=b""
        if destination.name=="base-identity":
            stdout=adapter.encoded({"python":list(map(int,version.split("."))),"executable":str(python),"isolated":1,"dont_write_bytecode":True})+b"\n"
        elif destination.name=="create-venv":
            executable=environment/"venv/bin/python";executable.parent.mkdir(parents=True);executable.symlink_to(python)
        elif destination.name=="install":
            site=environment/"venv/lib"/("python"+minor)/"site-packages";site.mkdir(parents=True)
            for name,dist in (("isort","isort-9.0.2.dist-info"),("mypy_extensions","mypy_extensions-1.1.0.dist-info")):
                module=site/name/"__init__.py";module.parent.mkdir();module.write_bytes(b"# inert installed package\n")
                record=site/dist/"RECORD";record.parent.mkdir();record.write_bytes(b"inert recorded payload\n")
        elif destination.name=="package-identity":
            site=environment/"venv/lib"/("python"+minor)/"site-packages"
            distributions=[]
            for name,dist in (("isort","isort-9.0.2.dist-info"),("mypy_extensions","mypy_extensions-1.1.0.dist-info")):
                paths=[site/name/"__init__.py",site/dist/"RECORD"]
                distributions.append({"distribution_files":{p.relative_to(site).as_posix():adapter.capture.fact(p) for p in paths},"missing_distribution_files":[]})
            identity={**distributions[0],"version":"9.0.2","python":list(map(int,version.split("."))),
                "executable":str(environment/"venv/bin/python"),"prefix":str(environment/"venv"),"base_prefix":str(base),
                "isolated":1,"dont_write_bytecode":True,"user_site_enabled":False,"target_site":str(site),
                "module":str(site/"isort/__init__.py"),"installed_site_distributions":{"isort":"9.0.2","mypy-extensions":"1.1.0"},
                "installer_import_origin":str(installer)+"/pip/__init__.py","packaging_import_origin":str(installer)+"/pip/_vendor/packaging/__init__.py",
                "dependency_distributions":{"mypy-extensions":distributions[1]},
                "dependency_verification":{"dependencies_verified":True,"pinned_versions":{"isort":"9.0.2","mypy-extensions":"1.1.0"}}}
            if extra_distribution:identity["installed_site_distributions"]["pip"]="unapproved"
            if wrong_probe_origin:identity["packaging_import_origin"]="/ambient/packaging/__init__.py"
            stdout=adapter.encoded(identity)+b"\n"
        else:raise AssertionError("unexpected mocked preparation command")
        adapter.write_json(destination/"inputs.json",options["inputs"])
        for name,raw in (("stdin",b""),("stdout",stdout),("stderr",b"")):(destination/(name+".raw")).write_bytes(raw)
        receipt={"argv":argv,"cwd":str(cwd),"expected_exits":options["expected_exits"],"pid":321,"waited_pid":321,
            "wait_completed":True,"actual_exit":0,"timed_out":False,"launch_error":None,"passed":True,
            **{name+"_sha256":adapter.digest((destination/(name+(".json" if name=="inputs" else ".raw"))).read_bytes()) for name in ("stdin","stdout","stderr","inputs")}}
        adapter.write_json(destination/"receipt.json",receipt);return receipt
    arguments=(python,version,environment,pins[0],wheels[0],pins[1:],wheels[1:],interpreter_pin,installer_pin)
    return arguments,command


class IsortContractControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.lock,cls.requests,_=adapter.load_plan(REPO)

    def test_static_imports_and_observer_parse_do_not_import_targets(self):
        self.assertFalse({name.split(".")[0] for name in sys.modules}&{"isort","mypy","pydantic","pylint","astroid"})
        ast.parse(adapter.OBSERVER)
        ast.parse(adapter.MAC_PACKAGE_PROBE)

    def test_mac_preparer_uses_sealed_bundle_symlinks_without_ensurepip_for_all_targets(self):
        for version in adapter.VERSIONS.values():
            with self.subTest(version=version),tempfile.TemporaryDirectory() as temporary:
                arguments,fake=prepared_fixture(Path(temporary).resolve(),version);records=[]
                with patch.object(adapter,"run_command",side_effect=fake):executable=adapter.prepare_mac(*arguments,records)
                self.assertTrue(executable.is_symlink());self.assertEqual(len(records),4)
                for row in records:adapter.reconcile_process(row)
                creation=records[1]["receipt"]["argv"]
                self.assertEqual(creation[3:7],["-m","venv","--symlinks","--without-pip"])
                install=records[2]["receipt"]["argv"]
                self.assertEqual(install[3:6],["-c",adapter.mac_reference.INSTALLER,str(adapter.mac_reference.bundled_pip(arguments[0],version))])
                self.assertIn("--no-index",install);self.assertIn("--no-deps",install);self.assertIn("--no-compile",install)
                self.assertEqual(records[3]["receipt"]["argv"][5],install[5])
                self.assertNotIn("--copies",[arg for row in records for arg in row["receipt"]["argv"]])
                identity=json.loads((arguments[2]/"identity.json").read_bytes())
                before=adapter.payloads(arguments[2],identity,"before",arguments[0]);self.assertTrue(before["matches_identity"])
                after=adapter.payloads(arguments[2],identity,"after",arguments[0]);self.assertEqual(before,after)
                self.assertEqual((arguments[2]/identity["installer"]["retained"]).read_bytes(),Path(identity["installer"]["path"]).read_bytes())
                Path(identity["installer"]["path"]).write_bytes(b"late installer mutation")
                self.assertFalse(adapter.payloads(arguments[2],identity,"mutated",arguments[0])["matches_identity"])

    def test_mac_preparer_rejects_wrong_bundle_extra_distribution_and_unbound_probe_import(self):
        for option,count in (("wrong_installer",2),("extra_distribution",4),("wrong_probe_origin",4)):
            with self.subTest(option=option),tempfile.TemporaryDirectory() as temporary:
                arguments,fake=prepared_fixture(Path(temporary).resolve(),"3.13.5",**{option:True});records=[]
                with patch.object(adapter,"run_command",side_effect=fake),self.assertRaises(ValueError):adapter.prepare_mac(*arguments,records)
                self.assertEqual(len(records),count)
                self.assertTrue(all(row["capture_process_fact"]["wait_completed"] is True for row in records))
                self.assertFalse((arguments[2]/"identity.json").exists())

    def test_installer_target_and_archive_lock_drift_rejects_before_target_preparation(self):
        for mutation in ("archive","target"):
            lock=copy.deepcopy(self.lock)
            if mutation=="archive":lock["bundled_installers"]["3.13.5"]["archive_sha256"]="0"*64
            else:del lock["bundled_installers"]["3.14.0"]
            with patch.object(adapter,"checked_bytes",return_value=adapter.encoded(lock)),self.assertRaises(ValueError):adapter.load_plan(REPO)

    def test_failed_preparation_retains_waited_child_and_stops_before_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            arguments,fake=prepared_fixture(Path(temporary).resolve(),"3.13.5");records=[]
            def failed_creation(argv,cwd,destination,**options):
                returned=fake(argv,cwd,destination,**options)
                if destination.name=="create-venv":
                    returned["actual_exit"]=1;returned["passed"]=False
                    (destination/"stderr.raw").write_bytes(b"retained synthetic preparation failure\n")
                    returned["stderr_sha256"]=adapter.digest((destination/"stderr.raw").read_bytes())
                    (destination/"receipt.json").write_bytes(adapter.encoded(returned))
                return returned
            with patch.object(adapter,"run_command",side_effect=failed_creation),self.assertRaises(ValueError):adapter.prepare_mac(*arguments,records)
            self.assertEqual(len(records),2);self.assertEqual(records[1]["status"],"failed")
            self.assertEqual(records[1]["capture_process_fact"],{"pid":321,"waited_pid":321,"wait_completed":True,"actual_exit":1,"timed_out":False,"launch_error":None})
            self.assertTrue((arguments[2]/"create-venv/returned-receipt.json").is_file())
            self.assertEqual((arguments[2]/"create-venv/stderr.raw").read_bytes(),b"retained synthetic preparation failure\n")
            self.assertFalse((arguments[2]/"install").exists())

    def test_all_504_full_contexts_keep_entry_api_target_and_alias_bindings(self):
        self.assertEqual(len(self.requests),504)
        self.assertEqual(len({r["request_id"] for r in self.requests}),504)
        for target in adapter.VERSIONS:
            rows=[r for r in self.requests if r["target"]==target]
            self.assertEqual(len(rows),168)
            self.assertEqual(sum(r["operation"]=="isort_code_API" for r in rows),143)
            self.assertEqual(sum(r["operation"]=="isort_file_CLI" for r in rows),25)
        self.assertEqual(sum(len(r["expected"]) for r in self.requests),507)
        self.assertEqual(sum(len(r["expected"])==2 for r in self.requests),3)
        self.assertNotIn(b"/private/",(REPO/adapter.LOCK).read_bytes())

    def test_recipe_entry_and_target_changes_cannot_hide_behind_source_sha(self):
        for mutation in ("extension","selection","target_pointer"):
            lock=copy.deepcopy(self.lock)
            if mutation=="extension":lock["requests"][0]["api_recipe"]["extension"]="pyi"
            elif mutation=="selection":lock["requests"][0]["context_template"]["selection"]["paths"]=["other.py"]
            else:
                row=next(r for r in lock["requests"] if r["context_template"]["target"]=="3.12" and "literal_interactions.json" in r["expected_bindings"][0]["source"]["source_id"])
                row["expected_bindings"][0]["source"]["json_pointer"]=row["expected_bindings"][0]["source"]["json_pointer"].removesuffix("0")+"1"
            with self.subTest(mutation=mutation),patch.object(adapter,"checked_bytes",return_value=adapter.encoded(lock)),self.assertRaises(ValueError):
                adapter.load_plan(REPO)

    def test_every_api_expected_value_including_both_shared_aliases_is_compared(self):
        for request in self.requests:
            if request["operation"]!="isort_code_API":continue
            checks=adapter.compare_api(api_actual(request),request)
            self.assertTrue(all(item["equal"] for item in checks))
            self.assertTrue(all(item["exit_projection_is_process_exit"] is False for item in checks))

    def test_ast_address_variation_is_owner_kind_target_and_field_bounded(self):
        request=next(r for r in self.requests if r["target"]=="3.12" and r["expected"][0]["binding"]["source"]["source_id"]==adapter.BASE+"ast_errors.json")
        actual=api_actual(request)
        actual["message"]=actual["message"].split(" object at 0x",1)[0]+" object at 0xABCDEF>."
        self.assertTrue(all(item["equal"] for item in adapter.compare_api(actual,request)))
        for message in (actual["message"]+" extra",actual["message"].replace("<ast.Name","<ast.Call"),actual["message"].replace("0xABCDEF","0xNOTHEX")):
            changed=copy.deepcopy(actual);changed["message"]=message
            self.assertFalse(all(item["equal"] for item in adapter.compare_api(changed,request)))
        changed=copy.deepcopy(actual);changed["error_class"]="ValueError"
        self.assertFalse(all(item["equal"] for item in adapter.compare_api(changed,request)))
        later=next(r for r in self.requests if r["target"]=="3.14" and r["expected"][0]["binding"]["source"]["source_id"]==adapter.BASE+"ast_errors.json")
        changed=api_actual(later);changed["message"]+=" object at 0x123>."
        self.assertFalse(all(item["equal"] for item in adapter.compare_api(changed,later)))
        self.assertIsNone(adapter.address_message("prefix <ast.Constant object at 0x123>.",["Name"]))

    def test_api_byte_outputs_messages_and_streams_are_not_globally_normalized(self):
        request=next(r for r in self.requests if r["operation"]=="isort_code_API" and r["expected"][0]["value"].get("output") is not None)
        actual=api_actual(request);actual["output"]+="\r";actual["output_hex"]=actual["output"].encode().hex()
        self.assertFalse(all(item["equal"] for item in adapter.compare_api(actual,request)))
        actual=api_actual(request);actual["api_stderr_hex"]=b"warning\n".hex()
        with self.assertRaises(ValueError):adapter.compare_api(actual,request)

    def observe(self,temporary,request=None,**options):
        request=request or next(r for r in self.requests if r["operation"]=="isort_code_API")
        root=Path(temporary).resolve()/"context"
        with patch.object(adapter,"run_command",side_effect=capture_fake(request,**options)),patch.object(adapter,"module_identity"):
            result=adapter.execute(request,Path("/not-executed"),{},Path(temporary).resolve()/"environment",root)
        return result,root

    def test_one_api_call_retains_one_separate_actual_process_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            row,root=self.observe(temporary)
            self.assertEqual(row["errors"],[]);self.assertEqual(row["status"],"observed")
            self.assertEqual(adapter.process_counts([row]),{"actual_target_processes":1,"waited_target_processes":1,"verified_process_receipts":1,"actual_isort_code_calls":1})
            process=row["processes"][0]
            self.assertIs(type(process["returned_receipt"]["expected_exits"]),list)
            self.assertEqual(json.loads((root/"api/receipt.json").read_bytes()),process["receipt"])
            self.assertIs(row["issue_accepted"],False)

    def test_missing_corrupt_and_bool_tampered_saved_receipts_keep_known_children(self):
        for saved in ("missing","corrupt","bool_tamper"):
            with self.subTest(saved=saved),tempfile.TemporaryDirectory() as temporary:
                row,root=self.observe(temporary,saved=saved)
                self.assertEqual(row["status"],"failed")
                counts=adapter.process_counts([row]);self.assertEqual(counts["actual_target_processes"],1);self.assertEqual(counts["waited_target_processes"],1)
                self.assertEqual(counts["verified_process_receipts"],0);self.assertIsNone(row["actual_api_calls"])
                self.assertTrue((root/"api/returned-receipt.json").is_file())
                self.assertEqual(row["processes"][0]["returned_receipt"]["passed"],True)

    def test_file_cli_uses_exact_settings_src_flags_and_three_actual_stage_exits(self):
        request=next(r for r in self.requests if r["operation"]=="isort_file_CLI" and r["expected"][0]["binding"]["source"]["source_id"].endswith("insertion_markers.json"))
        with tempfile.TemporaryDirectory() as temporary:
            row,root=self.observe(temporary,request)
            self.assertEqual(row["errors"],[]);self.assertEqual(row["status"],"observed")
            self.assertEqual(len(row["processes"]),3);self.assertEqual(row["actual_api_calls"],0)
            self.assertEqual([p["receipt"]["actual_exit"] for p in row["processes"]],adapter.cli_expected(request)["exits"])
            for process in row["processes"]:
                argv=process["receipt"]["argv"]
                self.assertIn("--settings-file",argv);self.assertEqual(argv[argv.index("--settings-file")+1],".isort.cfg")
                self.assertEqual(argv[argv.index("--src"):argv.index("--src")+4],["--src",".","--src","./src"])
            self.assertEqual((root/"project/.isort.cfg").read_text(),adapter.settings(request["target"]))

    def test_public_cr_is_real_file_cli_and_never_the_private_code_api_output(self):
        request=next(r for r in self.requests if r["expected"][0]["binding"]["source"]["source_id"].endswith("public_cr_stream.json"))
        with tempfile.TemporaryDirectory() as temporary:
            row,root=self.observe(temporary,request)
            self.assertEqual(row["status"],"observed");self.assertEqual(row["actual_api_calls"],0)
            self.assertEqual((root/"project/main.py").read_bytes(),bytes.fromhex(request["source_hex"]))
        with tempfile.TemporaryDirectory() as temporary:
            row,_=self.observe(temporary,request,private_cr=True)
            self.assertEqual(row["status"],"failed")

    def test_failed_cli_receipt_keeps_raws_and_does_not_skip_later_stages(self):
        request=next(r for r in self.requests if r["operation"]=="isort_file_CLI")
        with tempfile.TemporaryDirectory() as temporary:
            row,root=self.observe(temporary,request,saved="corrupt")
            self.assertEqual(row["status"],"failed");self.assertEqual(len(row["processes"]),3)
            self.assertEqual(adapter.process_counts([row])["actual_target_processes"],3)
            self.assertTrue(all((root/name/"receipt.json").read_bytes()==b"{invalid" for name in adapter.STAGES))

    def test_changed_installed_record_is_retained_before_identity_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();environment=root/"environment";site=environment/"venv/lib/python3.12/site-packages"
            module=site/"isort/__init__.py";module.parent.mkdir(parents=True);module.write_bytes(b"# static package bytes\n")
            record=site/"isort-9.0.2.dist-info/RECORD";record.parent.mkdir();record.write_bytes(b"original RECORD\n")
            python=root/"base-python";python.write_bytes(b"static binary bytes");venv_python=environment/"venv/bin/python";venv_python.parent.mkdir();venv_python.write_bytes(python.read_bytes())
            identity={"module":str(module),"distribution_files":{"isort/__init__.py":adapter.capture.fact(module),"isort-9.0.2.dist-info/RECORD":adapter.capture.fact(record)},
                "dependency_distributions":{},"base_executable_sha256":adapter.digest(python.read_bytes()),"venv_executable_sha256":adapter.digest(python.read_bytes())}
            before=adapter.payloads(environment,identity,"before",python);self.assertTrue(before["matches_identity"])
            record.write_bytes(b"changed RECORD\n");after=adapter.payloads(environment,identity,"after",python)
            self.assertFalse(after["matches_identity"])
            retained=environment/after["package_files"]["isort-9.0.2.dist-info/RECORD"]["retained"]
            self.assertEqual(retained.read_bytes(),b"changed RECORD\n")

    def test_final_reread_rejects_late_corruption_against_original_capture_return(self):
        selected=[r for r in self.requests if r["target"]=="3.12"]
        request=next(r for r in selected if r["operation"]=="isort_code_API")
        for corruption in ("saved_bool_type","raw_and_both_receipts","memory_receipt","memory_returned_receipt"):
            with self.subTest(corruption=corruption),tempfile.TemporaryDirectory() as temporary:
                output=Path(temporary).resolve()/"run";output.mkdir()
                rows=adapter.planned_rows(selected);adapter.write_json(output/"initial-results.json",rows)
                initial=(output/"initial-results.json").read_bytes()
                root=output/"contexts"/request["request_id"]
                with patch.object(adapter,"run_command",side_effect=capture_fake(request)),patch.object(adapter,"module_identity"):
                    observed=adapter.execute(request,Path("/not-executed"),{},output/"environment",root)
                self.assertEqual(observed["status"],"observed")
                observation=(root/"observation.json").read_bytes()
                rows[selected.index(request)]=observed
                self.assertEqual(adapter.process_counts(rows)["verified_process_receipts"],1)
                if corruption.startswith("memory_"):
                    key="receipt" if corruption=="memory_receipt" else "returned_receipt"
                    observed["processes"][0][key]["passed"]=1
                elif corruption=="saved_bool_type":
                    saved=json.loads((root/"api/receipt.json").read_bytes());saved["passed"]=1
                    (root/"api/receipt.json").write_text(json.dumps(saved))
                else:
                    changed=b'{"changed":true}\n';(root/"api/stdout.raw").write_bytes(changed)
                    for name in ("receipt.json","returned-receipt.json"):
                        saved=json.loads((root/"api"/name).read_bytes());saved["stdout_sha256"]=adapter.digest(changed)
                        (root/"api"/name).write_text(json.dumps(saved))
                adapter.publish(output,rows,True)
                self.assertEqual(observed["status"],"failed")
                self.assertIs(observed["processes"][0]["final_proof_reconciled"],False)
                self.assertEqual(adapter.process_counts(rows),{"actual_target_processes":1,"waited_target_processes":1,"verified_process_receipts":0,"actual_isort_code_calls":1})
                self.assertEqual((root/"observation.json").read_bytes(),observation)
                self.assertEqual((output/"initial-results.json").read_bytes(),initial)
                self.assertEqual(json.loads((output/observed["result_pointer"]).read_bytes()),observed)
                self.assertEqual(json.loads((output/"final-results.json").read_bytes()),rows)
                self.assertEqual([json.loads(line) for line in (output/"raw-index.jsonl").read_bytes().splitlines()],rows)

    def test_host_rejection_keeps_intended_lane_initial_and_final_indexes_complete(self):
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary).resolve()/"run"
            with patch.object(adapter,"require_hosted",side_effect=ValueError("not hosted")),patch.object(adapter,"download") as download,patch.object(adapter,"prepare_mac") as prepare:
                code=adapter.run(REPO,output,Path("/never-executed"),"3.12.11")
            self.assertEqual(code,1);download.assert_not_called();prepare.assert_not_called()
            initial=(output/"initial-results.json").read_bytes();rows=json.loads((output/"final-results.json").read_bytes())
            self.assertEqual(len(rows),168);self.assertTrue(all(row["status"]=="not_run" for row in rows))
            self.assertEqual((output/"initial-results.json").read_bytes(),initial)
            self.assertTrue(all(json.loads((output/row["result_pointer"]).read_bytes())==row for row in rows))
            self.assertEqual([json.loads(line) for line in (output/"raw-index.jsonl").read_bytes().splitlines()],rows)


if __name__=="__main__":unittest.main()
