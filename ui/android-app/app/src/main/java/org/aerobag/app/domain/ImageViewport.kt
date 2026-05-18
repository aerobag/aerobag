package org.aerobag.app.domain

data class ImageViewportState(
    val leftPx: Float,
    val topPx: Float,
    val zoom: Float,
)

data class ImageDisplaySize(
    val widthPx: Float,
    val heightPx: Float,
)

fun clampImageZoom(zoom: Float): Float = zoom.coerceIn(1f, 8f)

fun createInitialImageViewport(
    imageWidthPx: Float,
    imageHeightPx: Float,
    viewportWidthPx: Float,
    viewportHeightPx: Float,
): ImageViewportState {
    val fitScale = minOf(viewportWidthPx / imageWidthPx, viewportHeightPx / imageHeightPx)
    val width = imageWidthPx * fitScale
    val height = imageHeightPx * fitScale
    return ImageViewportState(
        leftPx = (viewportWidthPx - width) / 2f,
        topPx = (viewportHeightPx - height) / 2f,
        zoom = 1f,
    )
}

fun imageDisplaySize(
    imageWidthPx: Float,
    imageHeightPx: Float,
    viewportWidthPx: Float,
    viewportHeightPx: Float,
    zoom: Float,
): ImageDisplaySize {
    val fitScale = minOf(viewportWidthPx / imageWidthPx, viewportHeightPx / imageHeightPx)
    val scale = fitScale * zoom
    return ImageDisplaySize(
        widthPx = imageWidthPx * scale,
        heightPx = imageHeightPx * scale,
    )
}

fun clampImageViewport(
    state: ImageViewportState,
    imageWidthPx: Float,
    imageHeightPx: Float,
    viewportWidthPx: Float,
    viewportHeightPx: Float,
    overscrollPx: Float,
): ImageViewportState {
    val size = imageDisplaySize(
        imageWidthPx = imageWidthPx,
        imageHeightPx = imageHeightPx,
        viewportWidthPx = viewportWidthPx,
        viewportHeightPx = viewportHeightPx,
        zoom = state.zoom,
    )
    val minLeft = viewportWidthPx - overscrollPx - size.widthPx
    val maxLeft = overscrollPx
    val minTop = viewportHeightPx - overscrollPx - size.heightPx
    val maxTop = overscrollPx
    return ImageViewportState(
        leftPx = clampToRange(state.leftPx, minLeft, maxLeft),
        topPx = clampToRange(state.topPx, minTop, maxTop),
        zoom = clampImageZoom(state.zoom),
    )
}

fun dragImageViewport(
    state: ImageViewportState,
    dxPx: Float,
    dyPx: Float,
    imageWidthPx: Float,
    imageHeightPx: Float,
    viewportWidthPx: Float,
    viewportHeightPx: Float,
    overscrollPx: Float,
): ImageViewportState = clampImageViewport(
    state = ImageViewportState(
        leftPx = state.leftPx + dxPx,
        topPx = state.topPx + dyPx,
        zoom = state.zoom,
    ),
    imageWidthPx = imageWidthPx,
    imageHeightPx = imageHeightPx,
    viewportWidthPx = viewportWidthPx,
    viewportHeightPx = viewportHeightPx,
    overscrollPx = overscrollPx,
)

fun zoomImageAroundPoint(
    state: ImageViewportState,
    anchorX: Float,
    anchorY: Float,
    nextZoom: Float,
    imageWidthPx: Float,
    imageHeightPx: Float,
    viewportWidthPx: Float,
    viewportHeightPx: Float,
    overscrollPx: Float,
): ImageViewportState {
    val zoom = clampImageZoom(nextZoom)
    val scale = zoom / state.zoom
    return clampImageViewport(
        state = ImageViewportState(
            leftPx = anchorX - (anchorX - state.leftPx) * scale,
            topPx = anchorY - (anchorY - state.topPx) * scale,
            zoom = zoom,
        ),
        imageWidthPx = imageWidthPx,
        imageHeightPx = imageHeightPx,
        viewportWidthPx = viewportWidthPx,
        viewportHeightPx = viewportHeightPx,
        overscrollPx = overscrollPx,
    )
}

private fun clampToRange(value: Float, min: Float, max: Float): Float {
    val lower = minOf(min, max)
    val upper = maxOf(min, max)
    return value.coerceIn(lower, upper)
}
