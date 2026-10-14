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

"""Export of models to VRML 97 form"""

import os.path
from PyRA2.MatrixComponents import TransformMatrix

def exportScene(scene, fd, materials=1, objects=1, lights=1, cameras=1,
                att_points=1, nav_type='EXAMINE', heed_rbcoll=1,
                convert_textures=1):
    """Export the scene object as VRML 97

    - scene is  a GMI model scene object

    - fd is the file descriptor to write to

    - The materials, objects, lights and cameras flags indicate wether or not
      to include those items in the export.

    - If att_points is true, attachment points are included as parameterized
      translation groups and the whole object tree is defined as a prototype.

    - nav_type is a VRML NavigationInfo type string; if None, no NavigationInfo
      node will be included.
    
    - If heed_rbcoll is true, use it to determine wether certain objects are
      included in the export; some objects are part of a scene only for the
      benefit of the Havoc engine, and serve no visual purpose.

    - If convert_textures is true, write PNG versions of used textures to the
      same directory as the file associated with fd. If the scene was imported
      without a directory context, or the fd is not a file, this will fail
      silently.

    """
    exporter = SceneExporter(fd, materials, objects, lights, cameras,
                             att_points, nav_type, heed_rbcoll,
                             convert_textures)
    exporter.exportScene(scene)

from PyRA2.VRMLExport import AsciiExporter

class SceneExporter(AsciiExporter):
    _materials = _objects = _lights = _cameras = _att_points = 1
    _nav_type = 'EXAMINE'
    _heed_rbcoll = _convert_textures = 1

    def __init__(self, fd, materials=1, objects=1, lights=1, cameras=1,
                 att_points=1, nav_type='EXAMINE', heed_rbcoll=1,
                 convert_textures=1):
        AsciiExporter.__init__(self, fd)
        self._materials = materials
        self._objects = objects
        self._lights = lights
        self._cameras = cameras
        self._nav_type = nav_type
        self._heed_rbcoll = heed_rbcoll
        self._convert_textures = convert_textures
        self._att_points = att_points

    def exportScene(self, scene):
        self.writelines('''#VRML V2.0 utf8
                        ''')
        self.open('WorldInfo')
        self.open('info', '[')
        self.writelines('''"VRML Export of a Robot Arena 2.0 GMI file",
                           "Exported by GMIModel.VRMLExport"''')
        self.close(']')
        self.writeline('title "%s"' % scene.getSourceFileName())
        self.close()
        self.writeline('')

        if self._nav_type:
            self.open('NavigationInfo')
            self.writeline('type "%s"' % self._nav_type)
            self.close()
            self.writeline('')
        
        if self._materials:
            exporter = MaterialsExporter(self._fd, scene.getBasePath(),
                                         self._convert_textures)
            self.writeline('# Materials')
            exporter.exportMaterials(scene.getMaterials())

        exporter = ObjectsExporter(self._fd, self._objects, self._lights,
                                   self._cameras, self._att_points,
                                   self._heed_rbcoll)
        exporter.exportObjects(scene)


class MaterialsExporter(AsciiExporter):
    _fd = None
    _base = ''
    _convert_textures = 1

    def __init__(self, fd, base, convert_textures=1):
        self._fd = fd
        self._base = base
        self._convert_textures = convert_textures

    def exportMaterials(self, materials, baseindex=''):
        index = 0
        for material in materials:
            self.exportMaterial(material, index, baseindex)
            index += 1

    def exportMaterial(self, m, index=0, baseindex=''):
        if m.getClass() == 'Multi/Sub-Object':
            base = '%s%02d_' % (baseindex, index)
            self.exportMaterials(m.getSubMaterials(), base)
        else:
            self.open('PROTO Material_%s%02d []' % (baseindex, index))
            self.writelines('''# Name: %s
                               # Wiresize   %i
                               # Shading    %s
                               # XP Falloff %f
                               # XP Type    %s
                               # Two sided  %i''' % (
                m.getName(),
                m.getWireSize(), m.getShading(), m.getTransparencyFalloff(),
                m.getTransparencyType(), m.getTwoSided()
                ))
            self.open('Appearance')

            self.open('material Material')
            specularColor = m.getSpecularColor()
            shine = m.getShineStrength()
            specularColor = tuple([f * shine for f in specularColor])
            self.writelines('''ambientIntensity %g
                               diffuseColor     %s
                               shininess        %g
                               specularColor    %s
                               transparency     %g''' % (
                m.getShine(),                      # ambientIntensity
                '%g %g %g' % m.getDiffuseColor(),  # diffuseColor
                shine,                             # shininess
                '%g %g %g' % specularColor,        # specularColor
                m.getTransparency()                # transparency
                ))
            self.close()

            self.exportTextures(m.getTextures())

            self.close()
            self.close()
            self.writeline('')

    def exportTextures(self, textures):
        if not len(textures):
            return
        tex = textures[0] # Only export the first texture.
        if tex.getClass() != 'Bitmap':
            self.writelines('''# First texture class: %s
                               # Additional texture count: %i''' % (
                tex.getClass(), len(textures) - 1))
            return

        self.writelines('''# Name: %s
                           # Amount      %g
                           # Type        %s
                           # Map type    %s
                           # Blur        %g
                           # Blur offset %g
                           # Noise       %g
                           # Noise size  %g
                           # Noise lvl   %i
                           # Noise phase %g
                           # Inverted    %i
                           # Filter      %s
                           # Channel     %i''' % (
            tex.getName(), tex.getAmount(), tex.getType(), tex.getMapType(),
            tex.getUVBlur(), tex.getUVBlurOffset(), tex.getNoiseAmount(),
            tex.getNoiseSize(), tex.getNoiseLevel(), tex.getNoisePhase(),
            tex.getInvert(), tex.getFilter(), tex.getChannel()))

        self.open('texture ImageTexture')
        self.writelines('''url     "%s.png"
                           repeatS %s
                           repeatT %s''' % (
            os.path.splitext(tex.getBitmapName())[0],
            tex.getUVTiling()[0] and 'TRUE' or 'FALSE',
            tex.getUVTiling()[1] and 'TRUE' or 'FALSE'))
        self.close()

        self.open('textureTransform TextureTransform')
        self.writelines('''rotation    %g
                           scale       %g %g
                           translation %g %g''' % (
            (tex.getUVAngle(),) + tex.getUVTiling() + tex.getUVOffset()))
        self.close()

        self.writeline('# Additional texture count: %i' % (
            len(textures) - 1))

        self.exportTextureMap(tex.getBitmapName())

    def exportTextureMap(self, filename):
        if (not self._convert_textures or not self._base or 
            not hasattr(self._fd, 'name')):
            return
        import Image
        im = Image.open(os.path.join(self._base, 'maps', filename))
        output_dir = os.path.dirname(os.path.abspath(self._fd.name))
        output_file = '%s.png' % os.path.splitext(filename)[0]
        im.save(os.path.join(output_dir, output_file), optimized=1)


class ObjectsExporter(AsciiExporter):
    _objects = _lights = _cameras = _heed_rbcoll = _att_points = 1

    def __init__(self, fd, objects=1, lights=1, cameras=1, att_points=1,
                 heed_rbcoll=1):
        AsciiExporter.__init__(self, fd)
        self._objects = objects
        self._lights = lights
        self._cameras = cameras
        self._heed_rbcoll = heed_rbcoll
        self._att_points = 1

    def exportObjects(self, scene):
        if self._cameras:
            self.writeline('# Cameras')
            for cam in scene.getObjects().getItemsByType('Camera'):
                self.exportCamera(cam)
        if self._lights:
            self.writeline('# Lights')
            for light in scene.getObjects().getItemsByType('Light'):
                self.exportLight(light)

        if self._objects:
            self.writeline('# Objects')
            self.exportGeometricObjects(scene)

    def exportCamera(self, cam):
        self.open('Viewpoint')
        tm = TransformMatrix(cam.getTransformationMatrix())
        x, y, z = tm.getTranslation()
        pos = -x, y, z
        x, y, z, a = tm.getRotation()
        rotation = -x, y, z, -a
        self.writelines('''# Type   %s
                           # Hither %g
                           # Yon    %g
                           # Near   %g
                           # Far    %g
                           # Tdist  %g
                           fieldOfView %g
                           orientation %g %g %g %g
                           position    %g %g %g
                           description "%s"''' % ((
            cam.getType(), cam.getHither(), cam.getYon(), cam.getNear(),
            cam.getFar(), cam.getTargetDistance(), cam.getFieldOfView()) +
            rotation + pos + (cam.getName(),)))
        self.close()
        self.writeline('')

    def exportLight(self, light):
        if light.getType() == 'Omni':
            self.open('PointLight')
            tm = TransformMatrix(light.getTransformationMatrix())
            x, y, z = tm.getTranslation()
            pos = -x, y, z
            if light.getUseFarAttenuation():
                self.writeline('attenuation      %g %g %g' % (
                    light.getTargetDistance(),
                    light.getAttenuationStart(), 
                    light.getAttenuationEnd()))
            self.writelines('''color            %g %g %g
                               intensity        %g
                               location         %g %g %g
                               on               %s''' % (
                light.getColor() + (light.getIntensity(),) + pos + (
                    light.getUseLight() and 'TRUE' or 'FALSE',)))
            self.close()
        else:
            self.writeline('# Light type %s' % light.getType())
        self.writeline('')

    def exportGeometricObjects(self, scene):
        att_points = scene.getObjects().getItemsByType('AttachmentPoint')
        if self._att_points:
            self.open('PROTO Component', '[')
            point_names = self.exportAttachmentPointInterfaces(att_points)
            self.close('] {')
            self.indent()

        hierarchy = self.determineHierarchy(scene)
        self.open('Group')
        self.open('children', '[')

        for object, children in hierarchy:
            self.exportGeometricObject(object, children, scene)

        if self._att_points:
            self.writeline('')
            self.exportAttachmentPoints(att_points)

        self.close(']')
        self.close()

        if self._att_points:
            self.close()
            self.writeline('')

            # Provide prototypes per attachment point
            self.exportAttachmentPointPrototypes(att_points, point_names)

            # Make it possible to view just this component standalone.
            self.writelines('''
                               Component {}''')

    def exportAttachmentPointInterfaces(self, att_points):
        point_names = []
        for point in att_points:
            if getattr(point, 'type', '') != 'attach': continue
            point_names.append('point%02d' % int(point.id))
        
        for name in point_names:
            self.writeline('field MFNode children_%s []' % name)

        return point_names

    def exportAttachmentPoints(self, att_points):
        for point in att_points:
            if getattr(point, 'type', '') != 'attach': continue
            self.open('Transform')
            name = 'point%02d' % int(point.id)
            self.writeline('# Attachmentpoint: %s' % name)
            tm = TransformMatrix(point.getTransformationMatrix())
            x, y, z, a = tm.getRotation()
            rotation = -x, y, z, -a # rotation mirrorred.
            x, y, z = tm.getTranslation()
            translation = -x, y, z
            self.writelines('''rotation         %g %g %g %g
                               translation      %g %g %g''' % (
                rotation + translation))
            
            self.open('children', '[')
            self.open('Transform')
            self.writeline('rotation 1 0 0 3.14159')
            self.writeline('children IS children_%s' % name)
            self.close()
            self.close(']')

            self.close()

            self.writeline('')

    def exportAttachmentPointPrototypes(self, att_points, point_names):
        for point in att_points:
            if getattr(point, 'type', '') != 'attach': continue
            name = 'point%02d' % int(point.id)
            self.open('PROTO %s' % name, '[')
            for point_name in point_names:
                if point_name == name: continue
                self.writeline('field MFNode %s []' % point_name)
            self.close('] {')
            self.indent()
            self.open('Transform')

            tm = TransformMatrix(point.getTransformationMatrix())
            x, y, z, a = tm.getRotation()
            rotation = -x, y, z, a # Rotation in the opposite direction
            x, y, z = tm.getTranslation()
            translation = x, -y, -z # Translation in the other direction
            self.writeline('rotation         %g %g %g %g' % rotation)

            self.open('children', '[')
            self.open('Transform')
            self.writeline('translation      %g %g %g' % translation)
            
            self.open('children', '[')
            self.open('Component')
            for point_name in point_names:
                if point_name == name: continue
                self.writeline('children_%s IS %s' % (point_name, point_name))
            self.close()

            self.close(']')
            self.close()

            self.close(']')
            self.close()
            self.close()

            self.writeline('')

    def determineHierarchy(self, scene):
        parents = {}
        hidden = ()
        obs = scene.getObjects().getItemsByType('GeometricObject')
        if self._heed_rbcoll:
            hidden = self.findHidden(scene)
            if len(hidden) == len(obs):
                hidden = () # All hidden cannot be right
        for ob in obs:
            name = ob.getName()
            if not name in hidden and name.find('collision') == -1:
                children = parents.setdefault(ob.getParent(), [])
                children.append(ob)
        hierarchy = []
        for ob in parents['']:
            hierarchy.append((ob, self.findChildren(ob, parents)))
        return tuple(hierarchy)

    def findHidden(self, scene):
        rbcolls = scene.getObjects().getItemsByType('RigidBodyCollection')
        if not rbcolls: return ()
        hidden = {}
        for coll in rbcolls:
            self.findHiddenObs(coll.getRigidBodies(), hidden)
        return hidden.keys()

    def findHiddenObs(self, rbs, hidden):
        for rb in rbs:
            if rb.getUseDisplayProxy():
                hidden[rb.getName()] = 1
            self.findHiddenObs(rb.getChildren(), hidden)

    def findChildren(self, ob, parents):
        children = []
        for child in parents.get(ob.getName(), ()):
            children.append((child, self.findChildren(child, parents)))
        return tuple(children)

    def exportGeometricObject(self, object, children, scene):
        self.open('Transform')
        self.writeline('# Name: %s' % object.getName())
        tm = TransformMatrix(object.getTransformationMatrix())
        x, y, z, a = tm.getRotation()
        rotation = -x, y, z, -a
        x, y, z, a = tm.getScaleOrientation()
        scaleRot = -x, y, z, -a
        x, y, z = tm.getTranslation()
        translation = -x, y, z
        self.writelines('''rotation         %g %g %g %g
                           scale            %g %g %g
                           scaleOrientation %g %g %g %g
                           translation      %g %g %g''' % (
            rotation + tm.getScale() + scaleRot + translation))
        self.writeline('')
        self.open('children', '[')

        self.exportMesh(object.getMesh(), scene)
        self.writeline('')
        
        for ob, subobs in children:
            self.exportGeometricObject(ob, subobs, scene)
            self.writeline('')
        self.close(']')
        self.close()

    def exportMesh(self, mesh, scene):
        if not mesh.getPoints() or mesh.getMaterialReference() == -1:
            return

        matIdx = mesh.getMaterialReference()
        material = scene.getMaterial(matIdx)
        if material.getClass() == 'Multi/Sub-Object':
            for i in range(material.getSubMaterialCount()):
                submat = material.getSubMaterial(i)
                matRef = '%02d_%02d' % (matIdx, i)
                self.writeShape(mesh, matRef, submat, face=i)
                if material.getTwoSided():
                    self.writeShape(mesh, matRef, submat, face=i, invert=1)
        else:
            matRef = '%02d' % matIdx
            self.writeShape(mesh, matRef, material)
            if material.getTwoSided():
                self.writeShape(mesh, matRef, material, invert=1)

    def writeShape(self, mesh, matRef, material, face=None, invert=0):
        self.open('Shape')
        if face is not None:
            self.writeline('# Face %d' % face)
        self.writeline('appearance Material_%s {}' % matRef)

        self.open('geometry IndexedFaceSet')
        self.writeline('ccw %s' % (invert and 'FALSE' or 'TRUE'))
        
        # Points
        self.open('coord Coordinate')
        self.open('point', '[')
        for x, y, z in mesh.getPoints():
            self.writeline('%g, %g, %g,' % (-x, y, z))
        self.close(']')
        self.close()

        # Faces
        faceIndexes = []
        self.open('coordIndex', '[')
        count = 0
        for a, b, c, faceidx in mesh.getFaces():
            if face is None or face == faceidx:
                self.writeline('%i, %i, %i, -1,' % (a, b, c))
                faceIndexes.append(count)
            count += 1
        self.close(']')

        # Texture coords
        if mesh.getTextureCoords():
            if material.getTextureCount():
                channel = mesh.getTextureChannel(
                    material.getTexture(0).getChannel() - 1)
            else:
                channel = mesh
            self.open('texCoord TextureCoordinate')
            self.open('point', '[')
            for point in channel.getTextureCoords():
                self.writeline('%g, %g,' % point[:2])
            self.close(']')
            self.close()

            self.open('texCoordIndex', '[')
            count = 0
            for a, b, c in channel.getTextureFaces():
                if count in faceIndexes:
                    self.writeline('%i, %i, %i, -1,' % (a, b, c))
                count += 1
            self.close(']')

        # Color vertices
        if mesh.getColors():
            self.open('color Color')
            self.open('color', '[')
            for r, g, b in mesh.getColors():
                self.writeline('%g %g %g,' % (r, g, b))
            self.close(']')
            self.close()

            self.open('colorIndex', '[')
            count = 0
            for a, b, c in mesh.getColorFaces():
                if count in faceIndexes:
                    self.writeline('%i, %i, %i, -1,' % (a, b, c))
                count += 1
            self.close(']')

        # Normals
        self.open('normal Normal')
        self.open('vector', '[')
        count = 0
        for x, y, z in mesh.getNormals():
            if count % 4: # Skip face normals
                self.writeline('%g, %g, %g,' % (-x, y, z))
            count += 1
        self.close(']')
        self.close()

        # Normal faces
        self.open('normalIndex', '[')
        count = 0
        for i in range(0, len(mesh.getNormals()) * 0.75, 3):
            if count in faceIndexes:
                self.writeline('%i, %i, %i, -1,' % (i, i + 1, i + 2))
            count += 1
        self.close(']')

        self.writeline('# Additional channels: %d' % (
            mesh.getTextureChannelCount() - 1))
        self.close()
        self.close()
