#!/usr/bin/env python
import json
from collections import OrderedDict

def codepoint_to_rs((key, value):
    return "\'\\u{{{:x}}}\' => {},".format(key, json.dumps(value))


def load_codepoint_names():
    iterator = (x.split("\t", 1) for x in open('NamesList.txt').read().split("\n"))
    for line in iterator:
        if 2 != len(line):
            continue
        try:
            key = int(line[0], 16)
        except ValueError:
            continue
        else:
            yield (key, line[1])


print("use phf;\n\npub static NAMES: phf::Map<char, &'static str> = phf_map! {")
for line in map(codepoint_to_rs, load_codepoint_names()):
    print("    " + line)
print("};")
