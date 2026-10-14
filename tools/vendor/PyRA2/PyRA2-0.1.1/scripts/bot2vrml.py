#!/usr/bin/python
# PyRA2: Python support for Robot Arena 2 file formats.
# Copyright (C) 2003  Martijn Pieters <pyra2@zopatista.com>
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
#
# This library is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
# Lesser General Public License for more details.
#
# You should have received a copy of the GNU Lesser General Public
# License along with this library; if not, write to the Free Software
# Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA

"""Export RA2 Robot file (.bot) to a VRML 97 file."""

from PyRA2.RA2Bot.Import import loadRobot
from PyRA2.RA2Bot.VRMLExport import exportRobot
import os.path

DEFAULT_COMP_PATH = '..\\Components\\'

def exportRobot2VRML(input, output, nav_examine=1, comp_path=DEFAULT_COMP_PATH):
    robot = loadRobot(input)
    nav_type = nav_examine and 'EXAMINE' or None

    baseName = robot.getName().replace(' ', '_').replace('.', '_')

    texName = '%s.png' % baseName
    texPath = os.path.dirname(os.path.abspath(output.name))
    texFile = open(os.path.join(texPath, texName), 'wb')

    if robot.getSnapshot():
        snapFile = '%s_snap.png' % baseName
        robot.getSnapshot().save(os.path.join(texPath, snapFile), optimize=1)

    exportRobot(robot, output, tex_fd=texFile, tex_filename=texName,
                nav_type=nav_type, comp_path=comp_path)

if __name__ == '__main__':
    import sys, getopt

    def usage(status=1):
        print """%s [options] inputfile

Convert .bot file in inputfile to a VRML file. If inputfile is '-', stdinput is
read instead.
Options:

 -h              This help message
--help

 -o file         Output VRML to output file instead of stdout. A snapshot and 
--output         texture file (if available) will be saved to the same 
                 directory.

 -c URI          Relative or absolute URI of component VRML files. Defaults to
--comppath       %s, but can be any URI accepted by a VRML viewer.

 -f              Free navigation; don't lock it to 'EXAMINE'
--free
""" % (sys.argv[0], DEFAULT_COMP_PATH)
        sys.exit(status)

    try:
        options, args = getopt.getopt(sys.argv[1:], 'ho:c:f', 
                                      ('help', 'output', 'free'))
    except getopt.GetoptError:
        usage()

    output = sys.stdout
    comp_path = DEFAULT_COMP_PATH
    nav_examine = 1
    for opt, val in options:
        if opt in ('-h', '--help'):
            usage(0)
        if opt in ('-o', '--output'):
            output = open(val, 'w')
        if opt in ('-c', '--comppath'):
            comp_path = val
        if opt in ('-f', '--free'):
            nav_examine = 0

    if len(args) != 1:
        usage()

    if args[0] == '-':
        input = sys.stdin
    else:
        input = open(args[0], 'rb')

    exportRobot2VRML(input, output, nav_examine, comp_path)
