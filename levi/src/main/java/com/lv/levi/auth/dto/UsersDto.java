package com.lv.levi.auth.dto;

import lombok.AllArgsConstructor;
import lombok.Builder;
import lombok.Data;
import lombok.NoArgsConstructor;

import java.time.LocalDate;
import java.util.UUID;

@Data
@AllArgsConstructor
@NoArgsConstructor
@Builder
public class UsersDto {

    private UUID id;
    private String username;
    private String firstName;
    private String lastName;
    private String phone;
    private String email;
    private String password;
    private String profilePicture;
    private LocalDate createAt;
    private LocalDate updateAt;
}
