package com.lv.levi;

import org.junit.jupiter.api.Test;
import org.springframework.modulith.core.ApplicationModules;
import org.springframework.modulith.docs.Documenter;

class ModularityTest {

    // This scans your package structure starting from LeViApplication
    ApplicationModules modules = ApplicationModules.of(LeViApplication.class);

    @Test
    void verifyModularity() {
        // 1. Verifies that there are no cyclic dependencies
        // 2. Verifies that 'internal' packages aren't accessed from outside
        modules.verify();
    }

    @Test
    void writeDocumentation() {
        new Documenter(modules)
                .writeModulesAsPlantUml()
                .writeIndividualModulesAsPlantUml()
                .writeModuleCanvases();
    }
}