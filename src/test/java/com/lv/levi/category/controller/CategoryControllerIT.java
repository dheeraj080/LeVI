//package com.lv.levi.category.controller;
//
//import com.fasterxml.jackson.databind.ObjectMapper;
//import com.lv.levi.category.dto.CategoryDTO;
//import com.lv.levi.category.entity.CategoryType;
//import org.junit.jupiter.api.Test;
//import org.springframework.beans.factory.annotation.Autowired;
//import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
//import org.springframework.boot.test.context.SpringBootTest;
//import org.springframework.http.MediaType;
//import org.springframework.security.test.context.support.WithMockUser;
//import org.springframework.test.web.servlet.MockMvc;
//
//import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.*;
//import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;
//
//@SpringBootTest
//@AutoConfigureMockMvc
//class CategoryControllerIT {
//
//    @Autowired
//    private MockMvc mockMvc;
//
//    @Autowired
//    private ObjectMapper objectMapper;
//
//    @Test
//    @WithMockUser(username = "testuser")
//    void createCategory_Returns201() throws Exception {
//        CategoryDTO dto = CategoryDTO.builder()
//                .name("Gym")
//                .type(CategoryType.DEBIT)
//                .icon("barbell")
//                .build();
//
//        mockMvc.perform(post("/api/v1/categories")
//                        .contentType(MediaType.APPLICATION_JSON)
//                        .content(objectMapper.writeValueAsString(dto)))
//                .andExpect(status().isCreated())
//                .andExpect(jsonPath("$.name").value("Gym"));
//    }
//
//    @Test
//    void unauthorizedAccess_Returns401() throws Exception {
//        mockMvc.perform(get("/api/v1/categories"))
//                .andExpect(status().isUnauthorized());
//    }
//}