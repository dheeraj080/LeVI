package com.lv.levi.category.controller;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.dto.CategoryDTO;
import com.lv.levi.category.service.CategoryService;
import jakarta.validation.Valid;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.annotation.AuthenticationPrincipal;
import org.springframework.web.bind.annotation.*;

import java.util.List;
import java.util.UUID;

@RestController
@RequestMapping("/api/v1/categories")
@RequiredArgsConstructor
public class CategoryController {

    private final CategoryService categoryService;

    @GetMapping
    public ResponseEntity<List<CategoryDTO>> getCategories(@AuthenticationPrincipal UUID userId) {
        return ResponseEntity.ok(categoryService.getAllCategoriesForUser(userId));
    }

    @PostMapping
    public ResponseEntity<CategoryDTO> createCategory(
            @Valid @RequestBody CategoryDTO dto,
            @AuthenticationPrincipal com.lv.levi.auth.entity.User user) { // Accept full User object

        if (user == null) {
            return new ResponseEntity<>(HttpStatus.UNAUTHORIZED);
        }

        // Extract the ID here
        return new ResponseEntity<>(categoryService.createCategory(dto, user.getId()), HttpStatus.CREATED);
    }

    @PutMapping("/{id}")
    public ResponseEntity<CategoryDTO> updateCategory(
            @PathVariable UUID id,
            @Valid @RequestBody CategoryDTO dto,
            @AuthenticationPrincipal UUID userId) {
        return ResponseEntity.ok(categoryService.updateCategory(id, dto, userId));
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> deleteCategory(
            @PathVariable UUID id,
            @AuthenticationPrincipal UUID userId) {
        categoryService.deleteCategory(id, userId);
        return ResponseEntity.noContent().build();
    }
}