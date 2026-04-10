package com.lv.levi.transaction.controller;

import com.lv.levi.auth.entity.User;
import com.lv.levi.transaction.dto.TransactionDTO;
import com.lv.levi.transaction.service.TransactionService;
import jakarta.validation.Valid;
import lombok.RequiredArgsConstructor;
import org.springframework.format.annotation.DateTimeFormat;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.annotation.AuthenticationPrincipal;
import org.springframework.web.bind.annotation.*;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;
import java.util.UUID;

@RestController
@RequestMapping("/api/v1/transactions")
@RequiredArgsConstructor
public class TransactionController {

    private final TransactionService transactionService;

    @PostMapping
    public ResponseEntity<TransactionDTO> create(
            @Valid @RequestBody TransactionDTO dto,
            @AuthenticationPrincipal UUID userId) {
        return new ResponseEntity<>(transactionService.createTransaction(dto, userId), HttpStatus.CREATED);
    }

    @GetMapping
    public ResponseEntity<List<TransactionDTO>> getAll(@AuthenticationPrincipal UUID userId) {
        return ResponseEntity.ok(transactionService.getUserTransactions(userId));
    }

    @GetMapping("/range")
    public ResponseEntity<List<TransactionDTO>> getByRange(
            @RequestParam @DateTimeFormat(iso = DateTimeFormat.ISO.DATE) LocalDate start,
            @RequestParam @DateTimeFormat(iso = DateTimeFormat.ISO.DATE) LocalDate end,
            @AuthenticationPrincipal UUID userId) {
        return ResponseEntity.ok(transactionService.getTransactionsByPeriod(userId, start, end));
    }

    @GetMapping("/total-expense")
    public ResponseEntity<BigDecimal> getTotalExpense(@AuthenticationPrincipal UUID userId) {
        return ResponseEntity.ok(transactionService.getTotalExpense(userId));
    }

    @GetMapping("/category/{categoryId}/expense")
    public ResponseEntity<BigDecimal> getCategoryExpense(
            @PathVariable UUID categoryId,
            @AuthenticationPrincipal UUID userId) {
        return ResponseEntity.ok(transactionService.getExpenseByCategory(userId, categoryId));
    }
}