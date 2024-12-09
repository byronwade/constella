import * as React from "react";
import { Sheet, SheetContent, SheetTrigger } from "@/components/ui/sheet";
import { cn } from "@/lib/utils";

interface SidebarProps extends React.HTMLAttributes<HTMLDivElement> {
	children: React.ReactNode;
	side?: "left" | "right";
	defaultCollapsed?: boolean;
	collapsed?: boolean;
	onCollapsedChange?: (collapsed: boolean) => void;
}

export function Sidebar({ children, side = "left", defaultCollapsed, collapsed, onCollapsedChange, className, ...props }: SidebarProps) {
	const [isCollapsed, setIsCollapsed] = React.useState(defaultCollapsed ?? false);
	const [isMobile, setIsMobile] = React.useState(false);

	React.useEffect(() => {
		const checkMobile = () => {
			setIsMobile(window.innerWidth <= 768);
		};
		checkMobile();
		window.addEventListener("resize", checkMobile);
		return () => window.removeEventListener("resize", checkMobile);
	}, []);

	React.useEffect(() => {
		if (collapsed !== undefined) {
			setIsCollapsed(collapsed);
		}
	}, [collapsed]);

	const handleCollapsedChange = React.useCallback(
		(value: boolean) => {
			setIsCollapsed(value);
			onCollapsedChange?.(value);
		},
		[onCollapsedChange]
	);

	if (isMobile) {
		return (
			<Sheet>
				<SheetTrigger asChild>
					<button className="fixed left-4 top-4 z-40 rounded-md p-2 text-primary-foreground">
						<svg width="15" height="15" viewBox="0 0 15 15" fill="none" xmlns="http://www.w3.org/2000/svg" className="h-5 w-5">
							<path d="M1.5 3C1.22386 3 1 3.22386 1 3.5C1 3.77614 1.22386 4 1.5 4H13.5C13.7761 4 14 3.77614 14 3.5C14 3.22386 13.7761 3 13.5 3H1.5ZM1 7.5C1 7.22386 1.22386 7 1.5 7H13.5C13.7761 7 14 7.22386 14 7.5C14 7.77614 13.7761 8 13.5 8H1.5C1.22386 8 1 7.77614 1 7.5ZM1 11.5C1 11.2239 1.22386 11 1.5 11H13.5C13.7761 11 14 11.2239 14 11.5C14 11.7761 13.7761 12 13.5 12H1.5C1.22386 12 1 11.7761 1 11.5Z" fill="currentColor" fillRule="evenodd" clipRule="evenodd" />
						</svg>
					</button>
				</SheetTrigger>
				<SheetContent side={side} className={cn("w-[300px] p-0", className)} {...props}>
					{children}
				</SheetContent>
			</Sheet>
		);
	}

	return (
		<div data-collapsed={isCollapsed} className={cn("relative flex h-full flex-col gap-4 p-4", isCollapsed && "w-[60px]", !isCollapsed && "w-[300px]", className)} {...props}>
			<button onClick={() => handleCollapsedChange(!isCollapsed)} className="absolute -right-3 top-10 z-40 flex h-6 w-6 items-center justify-center rounded-full border bg-background">
				<svg width="15" height="15" viewBox="0 0 15 15" fill="none" xmlns="http://www.w3.org/2000/svg" className={cn("h-4 w-4 transition-transform", isCollapsed && "rotate-180")}>
					<path d="M8.84182 3.13514C9.04327 3.32401 9.05348 3.64042 8.86462 3.84188L5.43521 7.49991L8.86462 11.1579C9.05348 11.3594 9.04327 11.6758 8.84182 11.8647C8.64036 12.0535 8.32394 12.0433 8.13508 11.8419L4.38508 7.84188C4.20477 7.64955 4.20477 7.35027 4.38508 7.15794L8.13508 3.15794C8.32394 2.95648 8.64036 2.94628 8.84182 3.13514Z" fill="currentColor" fillRule="evenodd" clipRule="evenodd" />
				</svg>
			</button>
			{children}
		</div>
	);
}
