import qupath.lib.regions.RegionRequest
import qupath.lib.regions.ImagePlane
import qupath.lib.roi.ROIs
import qupath.lib.objects.PathObjects
import qupath.lib.images.servers.ImageServers
import qupath.lib.io.PathIO
import qupath.lib.io.GsonTools

def ms = { long t0 -> String.format('%.1f', (System.nanoTime() - t0) / 1e6) }
def imageData = getCurrentImageData()
def server = imageData.getServer()
def uri = server.getURIs()[0]

// 1. Open: build a fresh server for the same file.
long t = System.nanoTime()
def s2 = ImageServers.buildServer(uri)
s2.getMetadata()
println "BENCH open_ms=${ms(t)}"
s2.close()

// 2. 200 cold full-resolution tiles, sequential.
def path = server.getPath()
t = System.nanoTime()
for (int ty = 0; ty < 10; ty++)
    for (int tx = 0; tx < 20; tx++)
        server.readRegion(RegionRequest.createInstance(path, 1.0, 1024 + tx * 512, 4096 + ty * 512, 512, 512))
println "BENCH tiles200_ms=${ms(t)}"

// 3. Positive cell detection, same parameters as qupath-rs, twice per region.
setImageType('BRIGHTFIELD_H_DAB')
setColorDeconvolutionStains('{"Name" : "H-DAB default", "Stain 1" : "Hematoxylin", "Values 1" : "0.65 0.70 0.29", "Stain 2" : "DAB", "Values 2" : "0.27 0.57 0.78", "Background" : " 255 255 255"}')
def params = '{"detectionImageBrightfield": "Hematoxylin OD", "requestedPixelSizeMicrons": 0.5, "backgroundRadiusMicrons": 8.0, "medianRadiusMicrons": 0.0, "sigmaMicrons": 1.5, "minAreaMicrons": 10.0, "maxAreaMicrons": 400.0, "threshold": 0.1, "maxBackground": 2.0, "watershedPostProcess": true, "cellExpansionMicrons": 5.0, "includeNuclei": true, "smoothBoundaries": true, "makeMeasurements": true, "thresholdCompartment": "Nucleus: DAB OD mean", "thresholdPositive1": 0.2, "thresholdPositive2": 0.4, "thresholdPositive3": 0.6, "singleThreshold": true}'
for (def spec in [[2000, 'small'], [6000, 'large']]) {
    int size = spec[0]
    for (int run = 1; run <= 2; run++) {
        clearAllObjects()
        def ann = PathObjects.createAnnotationObject(ROIs.createRectangleROI(12000 - size / 2, 9000 - size / 2, size, size, ImagePlane.getDefaultPlane()))
        addObject(ann)
        selectObjects(ann)
        t = System.nanoTime()
        runPlugin('qupath.imagej.detect.cells.PositiveCellDetection', params)
        def cells = getCellObjects()
        def pos = cells.count { it.getPathClass()?.toString() == 'Positive' }
        println "BENCH cells_${spec[1]}_run${run}_ms=${ms(t)} n=${cells.size()} positive=${pos} meas=${cells[0].getMeasurementList().size()}"
    }
}

// 4. GeoJSON export of the large result.
def out = new File(args[0])
t = System.nanoTime()
exportAllObjectsToGeoJson(out.getAbsolutePath(), "FEATURE_COLLECTION")
println "BENCH geojson_export_ms=${ms(t)} bytes=${out.length()}"
t = System.nanoTime()
def back = PathIO.readObjects(out)
println "BENCH geojson_import_ms=${ms(t)} n=${back.size()}"
