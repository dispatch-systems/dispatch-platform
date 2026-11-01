package com.dspdispatch.relay.setup

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MakerScreensTest {
    private val makers = listOf(
        "samsung", "xiaomi", "redmi", "poco", "huawei", "honor", "oppo", "realme", "oneplus", "vivo", "iqoo",
        "asus", "lenovo", "motorola", "tecno", "infinix", "itel", "meizu", "letv", "htc", "hmd global",
    )

    @Test
    fun `finds a maker's screens whatever the case Android gives its name in`() {
        assertEquals(MakerScreens.candidates("samsung"), MakerScreens.candidates(" Samsung "))
        assertTrue(MakerScreens.candidates("Xiaomi").first().startsWith("com.miui.securitycenter/"))
    }

    @Test
    fun `has none for a maker that adds no screen of its own`() {
        assertEquals(emptyList<String>(), MakerScreens.candidates("Google"))
    }

    @Test
    fun `names each screen as package and class`() {
        makers.flatMap(MakerScreens::candidates).forEach { assertTrue(it, Regex("[\\w.]+/[\\w.]+").matches(it)) }
    }

    @Test
    fun `can see each maker's package, as the manifest's queries let it`() {
        val manifest = File("src/main/AndroidManifest.xml").readText()
        val queried = Regex("<package android:name=\"([^\"]+)\"").findAll(manifest).map { it.groupValues[1] }.toSet()
        val needed = makers.flatMap(MakerScreens::candidates).map { it.substringBefore('/') }.toSet()
        assertEquals(needed, queried)
    }
}
