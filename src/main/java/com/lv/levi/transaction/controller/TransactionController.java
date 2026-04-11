package com.lv.levi.transaction.controller;

import com.lv.levi.transaction.dto.TransactionDTO;
import com.lv.levi.transaction.dto.TransactionSummaryDTO;
import com.lv.levi.transaction.service.TransactionService;
import jakarta.validation.Valid;
import lombok.RequiredArgsConstructor;
import org.springframework.format.annotation.DateTimeFormat;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.annotation.AuthenticationPrincipal;
import org.springframework.web.bind.annotation.*;
import com.lv.levi.auth.UserPrincipal;

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
            @AuthenticationPrincipal UserPrincipal principal) {
        return new ResponseEntity<>(transactionService.createTransaction(dto, principal.id()), HttpStatus.CREATED);
    }

    @GetMapping
    public ResponseEntity<List<TransactionDTO>> getAll(@AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(transactionService.getUserTransactions(principal.id()));
    }

    @GetMapping("/range")
    public ResponseEntity<List<TransactionDTO>> getByRange(
            @RequestParam @DateTimeFormat(iso = DateTimeFormat.ISO.DATE) LocalDate start,
            @RequestParam @DateTimeFormat(iso = DateTimeFormat.ISO.DATE) LocalDate end,
            @AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(transactionService.getTransactionsByPeriod(principal.id(), start, end));
    }

    @GetMapping("/total-expense")
    public ResponseEntity<BigDecimal> getTotalExpense(@AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(transactionService.getTotalExpense(principal.id()));
    }

    @GetMapping("/summary")
    public ResponseEntity<TransactionSummaryDTO> getSummary(@AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(transactionService.getTransactionSummary(principal.id()));
    }

    @PutMapping("/{id}")
    public ResponseEntity<TransactionDTO> update(
            @PathVariable UUID id,
            @Valid @RequestBody TransactionDTO dto,
            @AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(transactionService.updateTransaction(id, dto, principal.id()));
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> delete(
            @PathVariable UUID id,
            @AuthenticationPrincipal UserPrincipal principal) {
        transactionService.deleteTransaction(id, principal.id());
        return ResponseEntity.noContent().build();
    }

    @GetMapping("/category/{categoryId}/expense")
    public ResponseEntity<BigDecimal> getCategoryExpense(
            @PathVariable UUID categoryId,
            @AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(transactionService.getExpenseByCategory(principal.id(), categoryId));
    }
}