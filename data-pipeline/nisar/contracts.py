"""Read authoritative contracts without modifying shared files or branch state."""
import json
import subprocess
import re
from datetime import datetime
from jsonschema import Draft202012Validator, FormatChecker
from common import ROOT

FORMATS = FormatChecker()

@FORMATS.checks('date-time', raises=ValueError)
def date_time(value):
    # jsonschema's optional date-time dependency is absent in the minimal environment.
    if not isinstance(value, str):
        return True
    pattern = r'\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})'
    return re.fullmatch(pattern, value) is not None and datetime.fromisoformat(value.upper().replace('Z', '+00:00')).tzinfo is not None

def git_text(path):
    return subprocess.check_output(['git', 'show', 'origin/main:'+path], cwd=ROOT.parent).decode('utf-8')

def schema(name):
    local = ROOT.parent / 'contracts' / 'v1' / name
    return json.loads(local.read_text(encoding='utf-8') if local.exists() else git_text('contracts/v1/'+name))

def validate(name, data):
    Draft202012Validator(schema(name), format_checker=FORMATS).validate(data)
