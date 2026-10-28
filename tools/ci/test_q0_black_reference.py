"""Pure capture controls; no real Black or other reference tool is imported."""
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

import q0_black_reference as runner


def case(index=0):
    source = "x=1\n"
    return {"id":str(index),"source":source,"source_sha256":runner.digest(source.encode()),"extension":"py",
            "origins":[{"path":"upstream/control.py","sha256":"a"*64,"original_flags":""}]}


def success(output="x=1\n", hint=None):
    return {"output":output,"sha256":runner.digest(output.encode()),"reference_converges":True,
            "target_syntax_hint":hint,"reference_check_exit":int(output!="x=1\n"),"newline":"\n"}


def frozen(input_case, record=None):
    return {**input_case,"expected":[record or success() for _ in range(3)]}


class Capture(unittest.TestCase):
    def test_environment_snapshot_never_records_private_values(self):
        values = {"PATH":"/bin", "PYTHONHASHSEED":"random", "GITHUB_TOKEN":"secret-value",
                  "ACTIONS_RUNTIME_TOKEN":"another-secret", "UNRELATED":"private"}
        snapshot = runner.environment_snapshot(values)
        encoded = json.dumps(snapshot)
        self.assertEqual(snapshot["names"],sorted(values))
        self.assertEqual(snapshot["allowlisted_values"],{"PATH":"/bin","PYTHONHASHSEED":"random"})
        self.assertNotIn("secret-value",encoded)
        self.assertNotIn("another-secret",encoded)
        self.assertEqual(snapshot["private_value_identities"]["GITHUB_TOKEN"]["sha256"],runner.digest(b"secret-value"))

    def test_main_inherits_cwd_and_env_and_retains_exact_stdin(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)/"capture"
            raw = b'[{"source": "x=1\\n"}]'
            command = [sys.executable,"-c","import os,sys; sys.stdout.buffer.write(sys.stdin.buffer.read()); sys.stderr.write(os.environ['BLACK_CAPTURE_CONTROL'])"]
            original_popen = subprocess.Popen
            with patch.dict(os.environ,{"BLACK_CAPTURE_CONTROL":"inherited"}), patch.object(runner.subprocess,"Popen",wraps=original_popen) as launch:
                result = runner.run_inherited(command,raw,output,{"control":True})
            runner.require_receipt(result)
            self.assertNotIn("env",launch.call_args.kwargs)
            self.assertNotIn("cwd",launch.call_args.kwargs)
            self.assertEqual(result["argv"],command)
            self.assertEqual(result["cwd"],os.getcwd())
            self.assertEqual((output/"stdin.raw").read_bytes(),raw)
            self.assertEqual((output/"stdout.raw").read_bytes(),raw)
            self.assertEqual((output/"stderr.raw").read_bytes(),b"inherited")
            self.assertEqual(result["stdin_sha256"],runner.digest(raw))
            self.assertEqual(result["pid"],result["waited_pid"])

    def test_timeout_preserves_first_output_and_actual_wait(self):
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)/"capture"
            result=runner.run_inherited([sys.executable,"-c","import time; print('before',flush=True); time.sleep(30)"],b"[]",output,{},timeout=.5)
            self.assertTrue(result["timed_out"])
            self.assertTrue(result["wait_completed"])
            self.assertEqual(result["pid"],result["waited_pid"])
            self.assertNotEqual(result["actual_exit"],0)
            self.assertEqual((output/"stdout.raw").read_bytes(),b"before\n")
            with self.assertRaises(ValueError):runner.require_receipt(result)
            with self.assertRaises(FileExistsError):runner.run_inherited([sys.executable,"-c","pass"],b"",output,{})
            self.assertTrue(json.loads((output/"receipt.json").read_bytes())["timed_out"])

    def test_launch_failure_and_nonzero_batch_exit_do_not_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            missing=runner.run_inherited([str(root/"absent")],b"[]",root/"missing",{})
            self.assertIsNone(missing["pid"])
            self.assertFalse(missing["wait_completed"])
            self.assertIsNotNone(missing["launch_error"])
            nonzero=runner.run_inherited([sys.executable,"-c","raise SystemExit(123)"],b"[]",root/"nonzero",{})
            self.assertEqual(nonzero["actual_exit"],123)
            for receipt in [missing,nonzero]:
                with self.assertRaises(ValueError):runner.require_receipt(receipt)


class Records(unittest.TestCase):
    def test_exact_transport_and_record_field_order(self):
        cases=[case(0),case(1)]
        records=[success(),{"reference_error":"InvalidInput: control","target_syntax_hint":None,"reference_check_exit":123}]
        with patch.object(runner,"CASE_COUNT",2):
            self.assertEqual(runner.decode_records(json.dumps(records).encode(),cases),records)
            for raw in [b"[]",json.dumps(records[:1]).encode(),json.dumps(records).encode()+b"\n"]:
                with self.assertRaises(ValueError):runner.decode_records(raw,cases)
            changed=[dict(reversed(list(records[0].items()))),records[1]]
            with self.assertRaises(ValueError):runner.decode_records(json.dumps(changed).encode(),cases)

    def test_computed_status_is_separate_from_shared_batch_exit(self):
        cases=[case(i) for i in range(3)]
        records=[success(),success("x = 1\n"),{"reference_error":"InvalidInput: control","target_syntax_hint":None,"reference_check_exit":123}]
        expected=[frozen(c,r) for c,r in zip(cases,records)]
        with patch.object(runner,"CASE_COUNT",3):
            rows=runner.make_observations(cases,expected,records,12,{"actual_exit":0},{"referenced_logs":[]})
        self.assertEqual([row["computed_reference_check_exit"] for row in rows],[0,1,123])
        self.assertTrue(all(row["observed_batch_actual_exit"]==0 and row["per_case_process_exit_observed"] is False for row in rows))
        self.assertTrue(all(row["strict_comparison"]["equal"] for row in rows))
        self.assertTrue(all(set(row["semantic_roles"].values())=={"unknown"} for row in rows))

    def test_primary_equality_and_secondary_metadata_remain_distinct(self):
        input_case=case()
        actual=success(hint="new syntax hint")
        with patch.object(runner,"CASE_COUNT",1):
            row=runner.make_observations([input_case],[frozen(input_case)], [actual],12,{"actual_exit":0},None)[0]
        self.assertTrue(row["strict_comparison"]["equal"])
        self.assertFalse(row["secondary_metadata_equal"])
        self.assertEqual(row["secondary_integrity_errors"],[])

    def test_unrun_and_partial_rows_never_become_passes(self):
        cases=[case(i) for i in range(2)];expected=[frozen(c) for c in cases]
        with patch.object(runner,"CASE_COUNT",2):
            rows=runner.make_observations(cases,expected,None,12,None,None)
            self.assertEqual([row["observation_status"] for row in rows],["not_run","not_run"])
            self.assertTrue(all(row["strict_comparison"]["equal"] is None for row in rows))
            with self.assertRaises(ValueError):runner.make_observations(cases,expected,[success()],12,None,None)

    def test_mismatch_and_bad_output_hash_are_retained(self):
        input_case=case();actual=success("other\n");actual["sha256"]="0"*64
        with patch.object(runner,"CASE_COUNT",1):
            row=runner.make_observations([input_case],[frozen(input_case)],[actual],12,{"actual_exit":0},None)[0]
        self.assertFalse(row["strict_comparison"]["equal"])
        self.assertTrue(row["secondary_integrity_errors"])


class PackageIdentity(unittest.TestCase):
    def setup_identity(self,temporary):
        root=Path(temporary);venv=root/"venv";site=venv/"lib/python3.12/site-packages";site.mkdir(parents=True)
        pins=[{"name":name,"version":version} for name,version in runner.PINS.items()]
        distributions,modules={},{}
        for pin in pins:
            name=pin["name"];canonical=name.replace("_","-");module=site/(name+".py");module.write_bytes(b"harmless identity control")
            metadata=site/(name+"-"+pin["version"]+".dist-info");metadata.mkdir();record=str(metadata.relative_to(site)/"RECORD")
            fact={"bytes":module.stat().st_size,"sha256":runner.digest(module.read_bytes())}
            stream=io.StringIO(newline="");writer=csv.writer(stream,lineterminator="\r\n")
            writer.writerow([module.name,"sha256="+base64.urlsafe_b64encode(bytes.fromhex(fact["sha256"])).rstrip(b"=").decode(),str(fact["bytes"])])
            writer.writerow([record,"",""]);raw=stream.getvalue().encode();record_fact={"bytes":len(raw),"sha256":runner.digest(raw)}
            distributions[canonical]={"name":name,"version":pin["version"],"requires_dist":[],"metadata_path":str(metadata),
                "distribution_files":{module.name:fact,record:record_fact},"missing_distribution_files":[],
                "installed_records":{record:{**record_fact,"bytes_b64":base64.b64encode(raw).decode()}}}
            modules[name]={"path":str(module),**fact}
        distributions["black"]["requires_dist"]=["click>=8","pytokens~=0.4.0"]
        markers={"python_full_version":"3.12.11","python_version":"3.12","sys_platform":"darwin","platform_system":"Darwin","platform_machine":"arm64"}
        identity={"python":[3,12,11],"isolated":1,"modules":modules,"distributions":distributions,
                  "installed_site_distributions":{name.replace("_","-"):version for name,version in runner.PINS.items()},
                  "dependency_verification":runner.validate_dependencies(pins,distributions,markers)}
        return identity,pins,venv

    def test_all_seven_pins_and_complete_record_payloads(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=self.setup_identity(temporary)
            runner.require_package_identity(identity,"3.12.11",pins,venv)

    def test_missing_wrong_version_or_extra_distribution_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=self.setup_identity(temporary)
            for change in ["missing","version","extra"]:
                altered=copy.deepcopy(identity)
                if change=="missing":del altered["distributions"]["pytokens"]
                elif change=="version":altered["distributions"]["pytokens"]["version"]="0.4.0"
                else:altered["installed_site_distributions"]["pip"]="25.0.1"
                with self.assertRaises(ValueError):runner.require_package_identity(altered,"3.12.11",pins,venv)

    def test_record_declared_payload_hash_is_checked_not_only_record_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=self.setup_identity(temporary)
            distribution=identity["distributions"]["black"];record=next(iter(distribution["installed_records"]))
            old=base64.b64decode(distribution["installed_records"][record]["bytes_b64"])
            rows=list(csv.reader(io.StringIO(old.decode())));rows[0][1]="sha256="+base64.urlsafe_b64encode(b"\0"*32).rstrip(b"=").decode()
            stream=io.StringIO(newline="");csv.writer(stream,lineterminator="\r\n").writerows(rows);raw=stream.getvalue().encode();fact={"bytes":len(raw),"sha256":runner.digest(raw)}
            distribution["distribution_files"][record]=fact
            distribution["installed_records"][record]={**fact,"bytes_b64":base64.b64encode(raw).decode()}
            with self.assertRaisesRegex(ValueError,"declared payload hash"):
                runner.require_package_identity(identity,"3.12.11",pins,venv)

    def test_missing_record_row_wrong_marker_and_module_tampering_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            identity,pins,venv=self.setup_identity(temporary)
            wrong=copy.deepcopy(identity);wrong["dependency_verification"]["marker_environment"]["sys_platform"]="linux"
            with self.assertRaisesRegex(ValueError,"marker"):runner.require_package_identity(wrong,"3.12.11",pins,venv)
            wrong=copy.deepcopy(identity);del wrong["distributions"]["black"]["distribution_files"]["black.py"]
            with self.assertRaisesRegex(ValueError,"denominator"):runner.require_package_identity(wrong,"3.12.11",pins,venv)
            Path(identity["modules"]["black"]["path"]).write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError,"module import"):runner.require_package_identity(identity,"3.12.11",pins,venv)

    def test_symlink_must_resolve_to_exact_base(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);base=root/"base";base.write_bytes(b"control");executable=root/"python";executable.symlink_to(base)
            self.assertTrue(runner.require_venv_link(executable,base)["venv_executable_is_symlink"])
            executable.unlink();executable.write_bytes(base.read_bytes())
            with self.assertRaises(ValueError):runner.require_venv_link(executable,base)
            executable.unlink();executable.symlink_to(root/"missing")
            with self.assertRaises((ValueError,FileNotFoundError)):runner.require_venv_link(executable,base)


class ObserverEvidence(unittest.TestCase):
    def setup_observer(self, root):
        identity,_,venv=PackageIdentity().setup_identity(root)
        identity["base_prefix"]="/pinned/base"
        runner.write_json(root/"identity.json",identity)
        executable=venv/"bin/python"
        cases=[case(0),case(1)]
        markers=["This diff might be helpful: ","This invalid output might be helpful: "]
        records,logs=[],[]
        (root/"referenced-error-logs").mkdir()
        canonical_temp=Path(tempfile.gettempdir()).resolve()
        for index,marker in enumerate(markers):
            with tempfile.NamedTemporaryFile(prefix="blk_",suffix=".log",delete=False) as stream:
                stream.write(b"harmless retained log "+str(index).encode())
                original=Path(stream.name)
            self.addCleanup(original.unlink,missing_ok=True)
            raw=original.read_bytes();saved=f"referenced-error-logs/control-{index}.raw"
            (root/saved).write_bytes(raw)
            records.append({"reference_error":"ASTSafetyError: control. "+marker+str(original),
                            "target_syntax_hint":None,"reference_check_exit":123})
            logs.append({"index":index,"id":cases[index]["id"],"error_class":"ASTSafetyError",
                         "marker":marker,"reported_path":str(original),"canonical_path":str(original.resolve()),
                         "canonical_temp_parent":str(canonical_temp),"saved_file":saved,
                         "bytes":len(raw),"sha256":runner.digest(raw)})
        observed={"python":[3,12,11],"executable":str(executable),"prefix":str(venv),
                  "base_prefix":identity["base_prefix"],"cwd":os.getcwd(),"black_version":"26.10.0",
                  "case_count":2,"result_count":2,"user_site_enabled":False,
                  "flags":{"isolated":0,"ignore_environment":0},"modules":{"black":identity["modules"]["black"]},
                  "canonical_temp_parent":str(canonical_temp),"referenced_logs":logs,"log_capture_errors":[]}
        return executable,cases,records,observed

    def validate(self,root,executable,cases,records,observed):
        (root/"observer-environment.json").write_text(json.dumps(observed))
        with patch.object(runner,"CASE_COUNT",2):
            return runner.validate_observer(root,executable,"3.12.11",cases,records)

    def test_both_raw_error_markers_require_their_exact_retained_logs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);executable,cases,records,observed=self.setup_observer(root)
            self.assertEqual(self.validate(root,executable,cases,records,observed),observed)
            no_logs=copy.deepcopy(observed);no_logs["referenced_logs"]=[]
            with self.assertRaisesRegex(ValueError,"coverage"):
                self.validate(root,executable,cases,records,no_logs)
            partial=copy.deepcopy(observed);partial["referenced_logs"].pop()
            with self.assertRaisesRegex(ValueError,"coverage"):
                self.validate(root,executable,cases,records,partial)

    def test_duplicate_unexpected_and_reused_saved_log_paths_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);executable,cases,records,observed=self.setup_observer(root)
            duplicate=copy.deepcopy(observed);duplicate["referenced_logs"].append(duplicate["referenced_logs"][0])
            unexpected=copy.deepcopy(observed);unexpected["referenced_logs"][0]["marker"]="unexpected"
            reused=copy.deepcopy(observed);reused["referenced_logs"][1]["saved_file"]=reused["referenced_logs"][0]["saved_file"]
            alias=copy.deepcopy(reused);alias["referenced_logs"][1]["saved_file"]="referenced-error-logs/./control-0.raw"
            for altered in [duplicate,unexpected,reused,alias]:
                with self.assertRaisesRegex(ValueError,"coverage"):
                    self.validate(root,executable,cases,records,altered)

    def test_retained_log_bytes_and_capture_failures_still_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);executable,cases,records,observed=self.setup_observer(root)
            altered=copy.deepcopy(observed);altered["log_capture_errors"]=[{"message":"unavailable"}]
            with self.assertRaisesRegex(ValueError,"missing or invalid"):
                self.validate(root,executable,cases,records,altered)
            (root/observed["referenced_logs"][0]["saved_file"]).write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError,"raw bytes"):
                self.validate(root,executable,cases,records,observed)


class Orchestration(unittest.TestCase):
    def test_complete_comparison_failure_and_actual_exit_failure_remain_failed(self):
        for actual_exit in [0,123]:
            with self.subTest(actual_exit=actual_exit),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);runner.write_json(root/"identity.json",{})
                input_case=case();records=[success("x = 1\n")]
                raw=json.dumps(records).encode()
                def capture(argv,stdin,output,inputs,timeout):
                    self.assertEqual(argv[:2],[str(root/"venv/bin/python"),"-c"])
                    self.assertEqual(timeout,300)
                    output.mkdir();(output/"stdout.raw").write_bytes(raw);(output/"stderr.raw").write_bytes(b"")
                    for name in ["environment-before.json","environment-after.json"]:(output/name).write_bytes(b"{}")
                    return {"pid":42,"waited_pid":42,"wait_completed":True,"timed_out":False,"launch_error":None,
                            "actual_exit":actual_exit,"passed":actual_exit==0}
                with (patch.object(runner,"CASE_COUNT",1),patch.object(runner,"run_inherited",side_effect=capture),
                      patch.object(runner,"validate_observer",return_value={"referenced_logs":[]})):
                    summary,rows=runner.capture_target(root/"venv/bin/python",root,"3.12.11",[input_case],
                                                       [frozen(input_case)],b"[]",{})
                self.assertEqual(summary["evidence_complete"],actual_exit==0)
                self.assertFalse(summary["strict_parity_passed"])
                self.assertFalse(summary["job_passed"])
                self.assertFalse(rows[0]["strict_comparison"]["equal"])
                self.assertEqual(rows[0]["observed_batch_actual_exit"],actual_exit)
                self.assertEqual(rows[0]["computed_reference_check_exit"],1)
                self.assertEqual((root/"main/stdout.raw").read_bytes(),raw)

    def test_all_three_targets_are_attempted_in_order_after_first_failure(self):
        repository=Path(__file__).resolve().parents[2];real_read=Path.read_bytes
        lockpath=repository/"tools/ci/q0_black_reference_lock.json";lock=json.loads(real_read(lockpath));cases=[case(i) for i in range(2)];expected=[frozen(c) for c in cases]
        raw=json.dumps(cases).encode();lock.update(case_count=2,input_bytes=len(raw),input_sha256=runner.digest(raw));lock["sdist"]["bytes"]=4
        def read(path):return json.dumps(lock).encode() if path==lockpath else real_read(path)
        def capture(executable,root,version,cases,expected,raw,shared):
            minor=int(version.split('.')[1]);rows=runner.make_observations(cases,expected,[success() for _ in cases],minor,{"actual_exit":0},None)
            return {"evidence_complete":True,"job_passed":True,"strict_parity_passed":True,"observed_rows":2,"indexed_rows":2},rows
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)/"run";argv=["runner","--repository",str(repository),"--output",str(output),"--python312","/base312","--python313","/base313","--python314","/base314"]
            with (patch.object(sys,"argv",argv),patch.object(runner,"CASE_COUNT",2),patch.object(Path,"read_bytes",read),
                  patch.object(runner,"require_host_context"),patch.object(runner,"download",return_value=b"fake"),
                  patch("q0_black_inputs.build_inputs",return_value=(cases,{})),patch("q0_black_inputs.read_expected",return_value=expected),
                  patch.object(runner,"prepare",side_effect=[ValueError("first failure"),Path("/p13"),Path("/p14")]) as prepare,
                  patch.object(runner,"capture_target",side_effect=capture) as capture_mock,patch("sys.stdout",new_callable=io.StringIO)):
                self.assertEqual(runner.main(),1)
            self.assertEqual([call.args[1] for call in prepare.call_args_list],list(runner.VERSIONS.values()))
            self.assertEqual(capture_mock.call_count,2)
            summary=json.loads((output/"summary.json").read_bytes());self.assertFalse(summary["job_passed"])
            self.assertEqual(summary["indexed_observations"],6)
            rows=[json.loads(line) for line in (output/"observation-index.jsonl").read_text().splitlines()]
            self.assertEqual([row["python_version"] for row in rows],["3.12.11"]*2+["3.13.5"]*2+["3.14.0"]*2)
            self.assertTrue(all(row["observation_status"]=="not_run" for row in rows[:2]))
            self.assertFalse(summary["issue_accepted"])
            self.assertEqual(summary["native_parity"],"not_run")


if __name__=="__main__":
    unittest.main()
