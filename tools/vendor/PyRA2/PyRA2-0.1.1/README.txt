This is PyRA2 version 0.1.1
===========================

Copyright (c) 2003 Martijn Pieters.
All rights reserved.


License information
-------------------

See the file "LICENSE" for terms & conditions for usage, and a DISCLAIMER OF ALL
WARRANTIES.

All trademarks referenced herein are property of their respective holders.


About
-----

PyRA2 is a set of Python packages (libraries) and documentation providing
access to the file formats of Robot Arena 2 (tm) (RA2), a game by Gabriel
Interactive (published by Infogrames). RA2 allows a player to design new robots
(consisting of a chassis and a set of components), and use these robots to
fight computer-controlled robots and, in a multi-player setting, the robot
designs of other players. For more information about this game, see
http://www.robotarena.com/.

Goals of PyRA2 are:

- Enabling a robot design exchange website for the player community, with VRML
  previews of available robots.
 
- Modification and creation of robot designs outside of RA2.
 
- Modification and creation of new components and arenas.

The current PyRA2 codebase can import both .gmf (3D models of robot components,
battle arenas and miscellaneous game items) and .bot files (robot body model and
component placement). It can also export both formats to VRML files for viewing
in a VRML browser/plugin.

An example of PyRA2's VRML export capabilities can be found at
http://www.zopatista.com/PyRA2Examples/bots where I placed exports of all robots
included with RA2, as well as designs by me and other RA2 players. Although this 
example covers the RA2 bots, PyRA2 can also import and export the game's arenas.


Further Documentation
---------------------

The docs subdirectory contains documentation about the RA2 3D file formats. The
fileformats were reverse-engineered; Gabriel Interactive has not released any
information about these formats.

Gabriel Interactive has released exporting code for 3D Studio Max to create new
arenas and components, including some documentation on how to build components
and arenas, at http://www.robotarena.com/exporter/.


Websites
--------

- Project: http://sf.net/projects/pyra2

- Download: http://sf.net/projects/showfiles.php?group_id=81504

- Demo: http://www.zopatista.com/PyRA2Examples/bots

- Robot Arena 2: http://www.robotarena.com/

- 3D Max Eporter: http://www.robotarena.com/exporter/


Requirements
------------

PyRA2 requires that the NumPy (http://sf.net/projects/numpy) and the Python
Imaging Library (PIL, http://www.pythonware.com/products/pil/) are installed
first.

This package has been tested with Python 2.2 and Python 2.3.


Installation
------------

On UNIX platforms simply run the following command in a terminal::

    python setup.py install

On Windows systems, open a command prompt window  or use the 'Run...' start menu 
option and execute the following command::

    python.exe setup.py install

You may have to pre-pend the path to the Python executable if the Python
directory is not part of your PATH.
