package com.lv.levi.auth.controller;

import com.lv.levi.auth.dto.UsersDto;
import com.lv.levi.auth.service.UserService;
import lombok.Getter;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;

@RestController
@RequiredArgsConstructor
@RequestMapping("/api/v1")
public class UsersController {

    private final UserService userService;

    @PostMapping("/register")
    public ResponseEntity<UsersDto> registerUsers(@RequestBody UsersDto usersDto) {
        UsersDto registeredUsers = userService.registerUser(usersDto);
        return ResponseEntity.status(HttpStatus.CREATED).body(registeredUsers);
    }

    @GetMapping("/activate")
    public ResponseEntity<String> activateUser(@RequestParam String code) {
        boolean isActivated = userService.activateUser(code);
        if (isActivated) {
            return ResponseEntity.status(HttpStatus.OK).body("Activated");
        } else  {
            return ResponseEntity.status(HttpStatus.BAD_REQUEST).body("Invalid activation code");
        }
    }
}
