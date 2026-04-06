package net.jonh.aerobag.prototype.domain

data class MapTileCell(
    val x: Int,
    val yTms: Int,
)

fun tileCells(view: MapTileView): List<MapTileCell> = buildList {
    for (dy in view.radius downTo -view.radius) {
        for (dx in -view.radius..view.radius) {
            add(
                MapTileCell(
                    x = view.centerX + dx,
                    yTms = view.centerYTms + dy,
                ),
            )
        }
    }
}

fun tileAssetPath(view: MapTileView, tile: MapTileCell): String =
    "tiles/${view.tileRoot}/${view.chartIndex}/${view.zoom}/${tile.x}/${tile.yTms}.webp"
