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

"""Export RA2 GMI file (.gmf) to a VRML 97 file."""

from PyRA2.GMIModel.Import import loadScene
from PyRA2.GMIModel.VRMLExport import exportScene

def exportGMI2VRML(input, output, show_hidden=0, nav_examine=1, cameras=0):
    scene = loadScene(input)
    nav_type = nav_examine and 'EXAMINE' or None
    exportScene(scene, output, cameras=cameras, nav_type=nav_type,
                heed_rbcoll=(not show_hidden))

if __name__ == '__main__':
    import sys, getopt

    def usage(status=1):
        print """%s [options] inputfile

Convert GMI file in inputfile to a VRML file. If inputfile is '-', stdinput is
read instead.
Options:

 -h              This help message
--help

 -o file         Output VRML to output file instead of stdout
--output

 -a              Include *all* objects in the GMI file (not just those that
--all            are defined in the Rigid Body collection as visible)

 -f              Free navigation; don't lock it to 'EXAMINE'
--free

 -c              Include camera nodes.
--cameras
""" % sys.argv[0]
        sys.exit(status)

    try:
        options, args = getopt.getopt(sys.argv[1:], 'ho:afc', 
                                      ('help', 'output', 'all', 'free',
                                       'cameras'))
    except getopt.GetoptError:
        usage()

    output = sys.stdout
    show_hidden = 0
    nav_examine = 1
    cameras = 0
    for opt, val in options:
        if opt in ('-h', '--help'):
            usage(0)
        if opt in ('-o', '--output'):
            output = open(val, 'w')
        if opt in ('-a', '--all'):
            show_hidden = 1
        if opt in ('-f', '--free'):
            nav_examine = 0
        if opt in ('-c', '--cameras'):
            cameras = 1

    if len(args) != 1:
        usage()

    if args[0] == '-':
        input = sys.stdin
    else:
        input = open(args[0], 'rb')

    exportGMI2VRML(input, output, show_hidden, nav_examine, cameras)
