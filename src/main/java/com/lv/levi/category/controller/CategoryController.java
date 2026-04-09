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
    public ResponseEntity<List<CategoryDTO>> getCategories(@AuthenticationPrincipal User user) {
        return ResponseEntity.ok(categoryService.getAllCategoriesForUser(user));
    }

    @PostMapping
    public ResponseEntity<CategoryDTO> createCategory(
            @Valid @RequestBody CategoryDTO dto,
            @AuthenticationPrincipal User user) {
        return new ResponseEntity<>(categoryService.createCategory(dto, user), HttpStatus.CREATED);
    }

    @PutMapping("/{id}")
    public ResponseEntity<CategoryDTO> updateCategory(
            @PathVariable UUID id,
            @Valid @RequestBody CategoryDTO dto,
            @AuthenticationPrincipal User user) {
        return ResponseEntity.ok(categoryService.updateCategory(id, dto, user));
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> deleteCategory(
            @PathVariable UUID id,
            @AuthenticationPrincipal User user) {
        categoryService.deleteCategory(id, user);
        return ResponseEntity.noContent().build();
    }
}