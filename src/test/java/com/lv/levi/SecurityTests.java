package com.lv.levi;

import com.lv.levi.auth.service.EmailService;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.security.test.context.support.WithMockUser;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

@SpringBootTest
@AutoConfigureMockMvc
public class SecurityTests {

    @Autowired
    private MockMvc mockMvc;

    // Satisfies the BREVO_SENDER_EMAIL dependency chain
    @MockitoBean
    private EmailService emailService;

    @Test
    public void accessUnsecuredResource_Success() throws Exception {
        mockMvc.perform(get("/api/public"))
                .andExpect(status().isOk());
    }

    @Test
    public void accessSecuredResource_Unauthenticated_401() throws Exception {
        mockMvc.perform(get("/api/private"))
                .andExpect(status().isUnauthorized());
    }

    @Test
    @WithMockUser(roles = "USER")
    public void accessSecuredResource_Authenticated_Success() throws Exception {
        mockMvc.perform(get("/api/private"))
                .andExpect(status().isOk());
    }

    @Test
    @WithMockUser(roles = "USER")
    public void accessAdminResource_AsUser_Forbidden() throws Exception {
        mockMvc.perform(get("/api/admin"))
                .andExpect(status().isForbidden());
    }

    @Test
    @WithMockUser(roles = "ADMIN")
    public void accessAdminResource_AsAdmin_Success() throws Exception {
        mockMvc.perform(get("/api/admin"))
                .andExpect(status().isOk());
    }
}