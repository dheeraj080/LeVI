//package com.lv.levi;
//
//import com.lv.levi.auth.entity.ResetPasswordToken;
//import com.lv.levi.auth.repository.ResetPasswordTokenRepository;
//import org.junit.jupiter.api.Test;
//import org.springframework.http.MediaType;
//import org.springframework.beans.factory.annotation.Autowired;
//import org.springframework.boot.test.context.SpringBootTest;
//import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
//import org.springframework.test.web.servlet.MockMvc;
//import static org.junit.jupiter.api.Assertions.*;
//
//import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
//import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;
//
//@SpringBootTest
//@AutoConfigureMockMvc
//class PasswordResetIntegrationTest {
//
//    @Autowired
//    private MockMvc mockMvc;
//
//    @Autowired
//    private ResetPasswordTokenRepository tokenRepository;
//
//
//
//    @Test
//    void shouldResetPasswordSuccessfully() throws Exception {
//        // 1. Request Reset
//        mockMvc.perform(post("/api/v1/auth/forgot-password")
//                        .contentType(MediaType.APPLICATION_JSON)
//                        .content("{\"email\": \"test@example.com\"}"))
//                .andExpect(status().isOk());
//
//        // 2. Safely grab token from DB
//        ResetPasswordToken resetToken = tokenRepository.findAll()
//                .stream()
//                .findFirst()
//                .orElseThrow(() -> new AssertionError("No tokens found in DB"));
//
//        String tokenValue = resetToken.getToken();
//
//        // 3. Perform Reset
//        mockMvc.perform(post("/api/v1/auth/reset-password")
//                        .contentType(MediaType.APPLICATION_JSON)
//                        .content("{\"token\": \"" + tokenValue + "\", \"newPassword\": \"NewSecurePass123!\"}"))
//                .andExpect(status().isOk());
//
//        // 4. Verify state
//        ResetPasswordToken updatedToken = tokenRepository.findByToken(tokenValue)
//                .orElseThrow();
//        assertTrue(updatedToken.isUsed(), "Token should be marked as used after reset");
//    }
//}
