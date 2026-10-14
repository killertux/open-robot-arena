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

"""Export a Bot to a VRML 97 description.

Components are linked in from external prototypes. The base URL for those
should be passed to the exporter if not local.

"""

def exportRobot(bot, fd, tex_fd=None, tex_filename=None, comp_path='',
                nav_type='EXAMINE', camera=1, light=1):
    """Export the Robot as VRML 97

    - bot is the robot model

    - fd is the file descriptor to write to

    - tex_fd, if defined, is the file descriptor to write the bot texturemap
      to.

    - tex_filename, if defined, will be used to link to the exported texture
      map; if not defined, but tex_fd is, tex_fd.name will be used instead.

    - comp_path is the base URL path to link to components.

    - nav_type is a VRML NavigationInfo type string; if None, no NavigationInfo
      node will be included.
    
    - If camera is true, a default viewpoint will be included that should show
      off the bot nicely

    - If light is true, a default set of lights will be included that should 
      show off the bot nicely
      
    The exported VRML file will include scripting that allows the viewer to see
    the internals of the robot by clicking on the chassis.

    """

    exporter = RobotExporter(fd, tex_fd, tex_filename, comp_path, nav_type,
                             camera, light)
    exporter.exportRobot(bot)


import os.path, math
from PyRA2.VRMLExport import AsciiExporter
from PyRA2.MatrixComponents import quatToAxisAndAngle
from ComponentReferences import COMPONENT_FILES

class RobotExporter(AsciiExporter):
    _tex_fd = None
    _tex_filename = None
    _comp_path = ''
    _nav_type = 'EXAMINE'
    _camera = 1
    _light = 1

    def __init__(self, fd, tex_fd=None, tex_filename=None, comp_path='', 
                 nav_type='EXAMINE', camera=1, light=1):
        AsciiExporter.__init__(self, fd)
        self._tex_fd = tex_fd
        self._tex_filename = tex_filename
        self._comp_path = comp_path
        self._nav_type = nav_type
        self._camera = camera
        self._light = light
    
    def exportRobot(self, bot):
        self.writelines('''#VRML V2.0 utf8
                        ''')
        self.open('WorldInfo')
        self.open('info', '[')
        self.writelines('''"VRML Export of a Robot Arena 2.0 robot file",
                           "Exported by RA2Bot.VRMLExport"''')
        self.close(']')
        self.writeline('title "%s"' % bot.getName())
        self.close()
        self.writeline('')

        if self._nav_type:
            self.open('NavigationInfo')
            self.writeline('type "%s"' % self._nav_type)
            self.close()
            self.writeline('')

        if self._camera:
            self.open('Viewpoint')
            self.writeline('position -1.0 1.5 4.0')
            self.writeline('orientation 0.4 0.4 0 -0.4')
            self.close()
            self.writeline('')

        if self._light:
            self.open('PointLight')
            self.writeline('location -1.5 1.5 3.0')
            self.close()
            self.writeline('')

            self.open('PointLight')
            self.writeline('location 1.0 1.5 3.0')
            self.close()
            self.writeline('')

            self.open('PointLight')
            self.writeline('location 1.0 1.5 -3.0')
            self.close()
            self.writeline('')

        self.exportComponentReferences(bot.getComponents())

        self.open('PROTO bot [ exposedField SFFloat bot_transparency 0 ]')
        self.exportChassis(bot)
        self.close()

        self.open('Transform')
        self.writeline('rotation 0 1 0 %g' % bot.getHeading())
        self.open('children', '[')
        self.writelines('''DEF the_bot bot {}
                           DEF touch TouchSensor {}''')
        self.open('DEF touchScript Script')
        self.writelines('''eventIn  SFTime  showOrHideComponents
                           eventOut SFFloat transparencyValue
                           field    SFBool  showComponents       FALSE

                           url "javascript:''')
        self.open('function showOrHideComponents(value)')
        self.open('if (showComponents)')
        self.writelines('''transparencyValue = 0;
                           showComponents = FALSE;''')
        self.close()
        self.open('else')
        self.writelines('''transparencyValue = 0.5;
                           showComponents = TRUE;''')
        self.close()
        self.close('}"')
        self.close()
        self.writelines('''
                ROUTE touch.touchTime TO touchScript.showOrHideComponents
                ROUTE touchScript.transparencyValue TO the_bot.bot_transparency
                ''')
        
        self.exportComponents(bot.getComponents(), 0)
        
        self.close(']')
        self.close()

    def exportComponentReferences(self, components):
        for i in range(1, len(components)):
            self.open('EXTERNPROTO Component%02d' % i, '[')

            # Determine attachment points used
            used_points = {}
            for child in components.getComponentsByParent(i):
                used_points[child.getParentMountPoint()] = 1
            
            # Write out fields
            for point in used_points.keys():
                self.writeline('field MFNode point%02d' % point)
            
            comp = components[i]
            if comp.getClass() == 'BurstMotor':
                self.writeline('field SFFloat axle_angle')

            file = COMPONENT_FILES[comp.getDataFile()]
            componentUrl = '%s%s' % (self._comp_path, file)
            self.close('] [ "%s#point%02d" ]' % (
                componentUrl, comp.getAttachmentPoint()))
            self.writeline('')

    def exportChassis(self, bot):
        chassis = bot.getChassis()
        
        # Mesh
        mesh = chassis.getMesh()
        
        self.open('Shape')
        self.open('geometry IndexedFaceSet')
        self.writeline('ccw FALSE')

        # Points
        self.open('coord Coordinate')
        self.open('point', '[')
        for x, y, z in mesh.getPoints():
            self.writeline('%g, %g, %g,' % (-x, y, z))
        self.close(']')
        self.close()

        # Faces
        self.open('coordIndex', '[')
        for a, b, c in mesh.getFaces():
            self.writeline('%i, %i, %i, -1,' % (a, b, c))
        self.close(']')

        # Texture coords
        self.open('texCoord TextureCoordinate')
        self.open('point', '[')
        for point in mesh.getTextureCoords():
            self.writeline('%g, %g,' % point)
        self.close(']')
        self.close()

        # Normals
        self.open('normal Normal')
        self.open('vector', '[')
        for x, y, z in mesh.getNormals():
            self.writeline('%g, %g, %g,' % (-x, y, z))
        self.close(']')
        self.close()

        self.close() # End geometry

        # Material
        if chassis.getTextureMap():
            textureName = self.exportTextureMap(chassis.getTextureMap())

            if textureName:
                self.open('appearance Appearance')
                self.open('material Material')
                self.writelines('''specularColor 0.5 0.5 0.5
                                   transparency  IS bot_transparency''')
                self.close()
                self.open('texture ImageTexture')
                self.writeline('url "%s"' % textureName)
                self.close()
                self.open('textureTransform TextureTransform')
                self.writeline('scale 1 -1')
                self.close()
                self.close()

        self.close()

    def exportTextureMap(self, map):
        if not self._tex_fd: return ''

        map.save(self._tex_fd, 'PNG', optimized=1)
        return self._tex_filename or self._tex_fd.name

    def exportComponents(self, components, parent):
        children = components.getComponentsByParent(parent)
        if parent != 0:
            groups = {}
            for child in children:
                group = groups.setdefault(child.getParentMountPoint(), [])
                group.append(child)
        else:
            groups = {0: children}

        for point, group in groups.items():
            for component in group:
                if parent != 0:
                    self.open('point%02d' % point, '[')
                self.open('Transform')
                x, y, z = component.getTranslation()
                translation = -x, y, z
                axis, a = quatToAxisAndAngle(component.getRotation()) 
                x, y, z = axis.tolist()
                rotation = -x, y, z, -a
                self.writelines('''translation %g %g %g
                                   rotation    %g %g %g %g''' % (
                    translation + rotation))
                self.open('children', '[')

                self.open('Transform')
                self.writelines('''translation 0 0 %g
                                   rotation 0 0 1 %g''' % (
                    -component.getHeight(), component.getAngle() + math.pi))
                self.open('children', '[')

                self.open('Component%02d' % component.getId())
                if component.getClass() == 'BurstMotor':
                    self.writeline('axle_angle %g' % (
                        component.getStartAngle()))
                self.exportComponents(components, component.getId())
                self.close()

                self.close(']')
                self.close()

                self.close(']')
                self.close()

                if parent != 0:
                    self.close(']')
