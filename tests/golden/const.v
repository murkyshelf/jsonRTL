module Four_bit_Constant_Output (
    value
);

output wire [3:0] value;

wire [3:0] value__2;

assign value = value__2;

assign value__2 = 4'b0011;

endmodule
