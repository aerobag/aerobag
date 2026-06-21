package org.aerobag.app.domain

@RequiresOptIn(
    level = RequiresOptIn.Level.ERROR,
    message = "Raw UI session work may page resources or run expensive core queries. Route it through UiSessionWorkRunner or another core-scheduled runner.",
)
@Retention(AnnotationRetention.BINARY)
@Target(AnnotationTarget.FUNCTION)
annotation class RawUiSessionWorkApi
