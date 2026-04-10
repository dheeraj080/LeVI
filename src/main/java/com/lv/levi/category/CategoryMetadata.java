package com.lv.levi.category;

// Using a 'record' automatically provides .name() and .icon() methods
public record CategoryMetadata(String name, String icon) {}